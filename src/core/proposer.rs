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
            // Check slots every 5 seconds
            sleep(Duration::from_millis(5000)).await;

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

                    println!("We are chosen proposer for block #{}! Proposing block with {} user transactions...", next_height, tx.kernels.len());
                    
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
                    };

                    // Sign the block header
                    let msg = header.hash();
                    header.validator_signature = Signature::sign(&msg, &private_key);

                    let block = Block {
                        header,
                        body: tx,
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
