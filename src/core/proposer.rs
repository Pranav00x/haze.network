use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use curve25519_dalek_ng::scalar::Scalar;
use bulletproofs::PedersenGens;

use super::mempool::Mempool;
use super::chain::{ChainState, ApplyResult};
use super::block::{Block, BlockHeader};
use super::storage::Storage;
use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use crate::p2p::server::P2pServer;
use crate::core::transaction::Transaction;

pub struct Proposer {
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
    storage: Arc<Storage>,
    stake_key: Option<Scalar>,
    p2p_server: Mutex<Option<Arc<P2pServer>>>,
}

impl Proposer {
    pub fn new(
        mempool: Arc<Mutex<Mempool>>,
        chain: Arc<Mutex<ChainState>>,
        storage: Arc<Storage>,
        stake_key: Option<Scalar>,
    ) -> Self {
        Self {
            mempool,
            chain,
            storage,
            stake_key,
            p2p_server: Mutex::new(None),
        }
    }

    /// Sets the P2P server instance to broadcast new blocks.
    pub fn set_p2p_server(&self, p2p_server: Arc<P2pServer>) {
        let mut server = self.p2p_server.lock().unwrap();
        *server = Some(p2p_server);
    }

    pub async fn start_proposing(&self) {
        if self.stake_key.is_none() {
            println!("Proposer started in passive validator mode (no staking key).");
            return;
        }
        let private_key = self.stake_key.unwrap();
        let gens = PedersenGens::default();
        let r_point = private_key * gens.B_blinding;

        println!("Staking proposer started. Monitoring slots...");

        loop {
            // Check slots every 10 seconds. A 1s interval only "worked"
            // because devnet has effectively one validator with ~0 network
            // latency - select_proposer itself needs no time (pure hash of
            // height+prev_hash), but the NEXT chosen proposer only has a
            // correct view once the previous block actually propagates to
            // them. With real geographically-distributed validators, an
            // interval shorter than gossip propagation + verification time
            // causes proposers to act on stale tips more often - more forks,
            // more orphaned blocks, less settled finality. 10s gives real
            // multi-validator networks comfortable room while still being
            // fast enough that "how do I show live progress" is a wallet
            // polling-cadence problem, not a protocol one.
            sleep(Duration::from_millis(10_000)).await;

            let (next_height, prev_hash, my_validator) = {
                let c = self.chain.lock().unwrap();
                let next_height = c.current_height + 1;
                let prev_hash = c.last_block_hash;

                // Find our matching validator commitment in active validators
                let mut my_val = None;
                for val in &c.active_validators {
                    let derived_r = val.commitment.as_point() - Scalar::from(val.value) * gens.B;
                    if derived_r == r_point {
                        my_val = Some(val.clone());
                        break;
                    }
                }

                // If validator set is empty and we possess the genesis staking key (Scalar(42)), we are the genesis proposer
                if my_val.is_none() && c.active_validators.is_empty() && private_key == Scalar::from(42u64) {
                    my_val = Some(crate::core::chain::Validator {
                        commitment: Commitment::new(1_000_000, private_key),
                        value: 1_000_000,
                    });
                }

                (next_height, prev_hash, my_val)
            };

            if let Some(validator) = my_validator {
                // Determine if we are the chosen proposer for this slot
                let chosen_proposer = {
                    let c = self.chain.lock().unwrap();
                    c.select_proposer(next_height, prev_hash)
                };

                if chosen_proposer == validator.commitment {
                    // Check for pending transactions or create empty block if none
                    let tx_bundle = {
                        let mut mp = self.mempool.lock().unwrap();
                        mp.aggregate()
                    };

                    let mut tx = tx_bundle.unwrap_or_else(|| Transaction {
                        inputs: vec![],
                        outputs: vec![],
                        kernels: vec![],
                    });

                    // Pull pending name registrations, filtering out any that would
                    // already fail against current chain state (name taken, or a
                    // fee-payment input already spent) - including one that would
                    // fail here would get the WHOLE block rejected by apply_linear_block,
                    // not just that one op.
                    let candidate_name_ops = {
                        let mut mp = self.mempool.lock().unwrap();
                        mp.take_name_ops()
                    };
                    let mut spent_this_block: std::collections::HashSet<Commitment> =
                        tx.inputs.iter().map(|i| i.commitment).collect();
                    let mut name_registry_snapshot = {
                        let c = self.chain.lock().unwrap();
                        c.name_registry.clone()
                    };
                    let utxos_snapshot = {
                        let c = self.chain.lock().unwrap();
                        c.utxos.clone()
                    };
                    let mut name_ops = Vec::new();
                    for op in candidate_name_ops {
                        let inputs_ok = op.fee_payment.inputs.iter()
                            .all(|i| utxos_snapshot.contains(&i.commitment) && !spent_this_block.contains(&i.commitment));
                        if !inputs_ok || name_registry_snapshot.contains_key(&op.name) {
                            continue;
                        }
                        for i in &op.fee_payment.inputs {
                            spent_this_block.insert(i.commitment);
                        }
                        name_registry_snapshot.insert(op.name.clone(), crate::core::registry::NameRecord {
                            name: op.name.clone(),
                            owner_pubkey: op.owner_pubkey,
                            resolves_to: op.resolves_to,
                            registered_at_block: next_height,
                        });
                        name_ops.push(op);
                    }
                    // Pull pending name transfers, filtering the same way: must target a
                    // name that already existed BEFORE this block (not one freshly
                    // registered above), can't collide with a name_op or another transfer
                    // in this same block, and must carry a valid signature from that
                    // name's current owner.
                    let candidate_transfer_ops = {
                        let mut mp = self.mempool.lock().unwrap();
                        mp.take_transfer_ops()
                    };
                    let original_registry = {
                        let c = self.chain.lock().unwrap();
                        c.name_registry.clone()
                    };
                    let mut names_touched: std::collections::HashSet<String> = name_ops.iter().map(|op| op.name.clone()).collect();
                    let mut transfer_ops = Vec::new();
                    for op in candidate_transfer_ops {
                        if names_touched.contains(&op.name) {
                            continue;
                        }
                        let Some(current) = original_registry.get(&op.name) else { continue };
                        let msg = crate::core::registry::TransferNameOp::signing_message(&op.name, &op.new_owner_pubkey, &op.new_resolves_to);
                        if !op.signature.verify(&msg, &current.owner_pubkey) {
                            continue;
                        }
                        names_touched.insert(op.name.clone());
                        name_registry_snapshot.insert(op.name.clone(), crate::core::registry::NameRecord {
                            name: op.name.clone(),
                            owner_pubkey: op.new_owner_pubkey,
                            resolves_to: op.new_resolves_to,
                            registered_at_block: current.registered_at_block,
                        });
                        transfer_ops.push(op);
                    }

                    let name_registry_root = crate::core::registry::compute_registry_root(&name_registry_snapshot);

                    println!("We are chosen proposer for block #{}! Proposing block with {} user transactions, {} name registrations, {} name transfers...", next_height, tx.kernels.len(), name_ops.len(), transfer_ops.len());
                    
                    // 1. Calculate total fees and coinbase value
                    let total_fees: u64 = tx.kernels.iter().map(|k| k.fee).sum();
                    let coinbase_value = super::block::BLOCK_REWARD + total_fees;

                    // 2. Generate random blinding factor for coinbase output
                    let mut rng = rand::thread_rng();
                    let r_coinbase = Scalar::random(&mut rng);

                    // 3. Create coinbase output and range proof
                    let coinbase_commitment = Commitment::new(coinbase_value, r_coinbase);
                    let coinbase_proof = crate::crypto::range_proof::RangeProof::prove(coinbase_value, &r_coinbase);
                    let coinbase_output = crate::core::transaction::Output {
                        commitment: coinbase_commitment,
                        proof: coinbase_proof,
                    };

                    // 4. Create coinbase kernel with additive inverse blinding factor: excess = -r_coinbase * H
                    let coinbase_excess_r = Scalar::zero() - r_coinbase;
                    let coinbase_excess_commitment = Commitment::new(0, coinbase_excess_r);
                    let coinbase_signature = Signature::sign(&0u64.to_le_bytes(), &coinbase_excess_r);
                    let coinbase_kernel = crate::core::transaction::TxKernel {
                        excess: coinbase_excess_commitment,
                        fee: 0,
                        signature: coinbase_signature,
                    };

                    // 5. Append coinbase output and kernel to block transaction body
                    tx.outputs.push(coinbase_output);
                    tx.kernels.push(coinbase_kernel);

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    let mut header = BlockHeader {
                        height: next_height,
                        prev_hash,
                        total_kernel_offset: Scalar::zero(),
                        nonce: 0,
                        timestamp: now,
                        validator_commitment: validator.commitment,
                        validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
                        name_registry_root,
                    };

                    // Sign the block header
                    let msg = header.hash();
                    header.validator_signature = Signature::sign(&msg, &private_key);

                    let block = Block {
                        header,
                        body: tx,
                        name_ops,
                        transfer_ops,
                    };

                    // Apply locally
                    let apply_result = {
                        let mut c = self.chain.lock().unwrap();
                        c.apply_block(&block)
                    };

                    match apply_result {
                        ApplyResult::Linear(delta) => {
                            println!("Block #{} successfully proposed and added to chain locally!", block.header.height);

                            // Save to disk
                            if let Err(e) = self.storage.persist_applied(&delta) {
                                println!("Warning: Failed to persist chain state to disk: {}", e);
                            }

                            // Broadcast block
                            let server_opt = {
                                let s = self.p2p_server.lock().unwrap();
                                s.clone()
                            };
                            if let Some(p2p) = server_opt {
                                let p2p_clone = Arc::clone(&p2p);
                                let block_clone = block.clone();
                                tokio::spawn(async move {
                                    p2p_clone.broadcast_block(block_clone).await;
                                });
                            }
                        }
                        _ => {
                            println!("Warning: Locally proposed block was rejected by local validation!");
                        }
                    }
                }
            }
        }
    }
}
