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

/// How long a client waits before trying a fallback proposer slot: if the
/// chain is still stuck at the same pending height after `rank * this`, the
/// validator at that rank in proposer_priority_order steps in instead of
/// waiting on a possibly-offline higher-priority validator forever (see
/// ChainState::proposer_priority_order - this is what actually makes use of
/// the freedom that fix gives, since consensus alone doesn't change which
/// validator attempts to propose first in the common case).
const FALLBACK_ROUND_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Proposer {
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
    storage: Arc<Storage>,
    stake_key: Option<Scalar>,
    p2p_server: Mutex<Option<Arc<P2pServer>>>,
    /// The pending height we're currently waiting on, and when we first
    /// started waiting on it - reset every time the chain tip advances.
    /// Used to time fallback-round eligibility.
    stalled_since: Mutex<Option<(u64, std::time::Instant)>>,
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
            stalled_since: Mutex::new(None),
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

                // If validator set is empty and we possess the genesis staking key
                // (see core::genesis::genesis_validator_blinding), we are the genesis proposer
                if my_val.is_none() && c.active_validators.is_empty() && private_key == crate::core::genesis::genesis_validator_blinding() {
                    my_val = Some(crate::core::chain::Validator {
                        commitment: Commitment::new(1_000_000, private_key),
                        value: 1_000_000,
                    });
                }

                (next_height, prev_hash, my_val)
            };

            if let Some(validator) = my_validator {
                // Track how long we've been waiting on this exact pending
                // height - resets the instant the tip advances to a new one.
                let stalled_for = {
                    let mut s = self.stalled_since.lock().unwrap();
                    let since = match *s {
                        Some((h, since)) if h == next_height => since,
                        _ => {
                            let now = std::time::Instant::now();
                            *s = Some((next_height, now));
                            now
                        }
                    };
                    since.elapsed()
                };

                // Our rank in this height's weighted priority order - rank 0
                // is the primary (identical to the old single-winner
                // select_proposer), rank N is the Nth fallback. We only
                // actually attempt to propose once we've been stuck long
                // enough for our rank (see FALLBACK_ROUND_TIMEOUT) - in the
                // common case (rank 0, chain not stalled) this behaves
                // exactly as before.
                let my_rank = {
                    let c = self.chain.lock().unwrap();
                    c.proposer_priority_order(next_height, prev_hash)
                        .iter()
                        .position(|commitment| *commitment == validator.commitment)
                };

                let eligible = match my_rank {
                    Some(0) => true,
                    Some(rank) => stalled_for >= FALLBACK_ROUND_TIMEOUT * rank as u32,
                    None => false,
                };

                if eligible {
                    if let Some(rank) = my_rank {
                        if rank > 0 {
                            println!("Primary proposer for block #{} appears offline (stuck for {:?}) - stepping in as fallback rank {}.", next_height, stalled_for, rank);
                        }
                    }
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

                    // Computed early (rather than just before BlockHeader
                    // construction, as before) since a collection mint's
                    // phase-timing gate below needs it too - this candidate
                    // block's own timestamp will be set to (approximately)
                    // this same value further down.
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    // Pull pending collection launches - same filtering
                    // pattern as name_ops above, separate namespace (see
                    // core::collections). No fee-payment/UTXO involvement to
                    // check (launches have none, by design).
                    let candidate_launch_ops = {
                        let mut mp = self.mempool.lock().unwrap();
                        mp.take_launch_collection_ops()
                    };
                    let mut collection_registry_snapshot = {
                        let c = self.chain.lock().unwrap();
                        c.collection_registry.clone()
                    };
                    let mut launch_collection_ops = Vec::new();
                    for op in candidate_launch_ops {
                        if op.validate_standalone().is_err() || collection_registry_snapshot.contains_key(&op.collection_id) {
                            continue;
                        }
                        collection_registry_snapshot.insert(op.collection_id.clone(), crate::core::collections::CollectionRecord {
                            collection_id: op.collection_id.clone(),
                            creator_pubkey: op.creator_pubkey,
                            name: op.name.clone(),
                            symbol: op.symbol.clone(),
                            metadata: op.metadata.clone(),
                            phases: op.phases.clone(),
                            launched_at_block: next_height,
                            royalty_bps: op.royalty_bps,
                        });
                        launch_collection_ops.push(op);
                    }
                    let collection_registry_root = crate::core::collections::compute_collection_registry_root(&collection_registry_snapshot);

                    // Pull pending asset mints/transfers - same filtering
                    // pattern as name_ops/transfer_ops above, separate
                    // namespace (see core::assets). A collection-tagged mint
                    // additionally needs the phase's timing/allowlist/quota
                    // rules satisfied - mirrors ChainState::apply_linear_block's
                    // gates exactly (this is soft/best-effort pre-filtering;
                    // apply_linear_block remains the sole hard consensus gate,
                    // so a bug here can only produce a wasted/rejected
                    // candidate block, never an invalid one that lands).
                    let candidate_mint_ops = {
                        let mut mp = self.mempool.lock().unwrap();
                        mp.take_mint_ops()
                    };
                    let mut asset_registry_snapshot = {
                        let c = self.chain.lock().unwrap();
                        c.asset_registry.clone()
                    };
                    let mut candidate_mint_counts = {
                        let c = self.chain.lock().unwrap();
                        c.collection_mint_counts.clone()
                    };
                    // Kernels this candidate block would itself add from its
                    // main body alone (before any name/mint fee-payment
                    // kernels are folded in below) - a collection mint's
                    // required_kernel_excess payment is an ordinary
                    // transaction that landed in tx via mempool.aggregate(),
                    // so it's already covered here.
                    let body_kernel_excesses: std::collections::HashSet<Commitment> =
                        tx.kernels.iter().map(|k| k.excess).collect();
                    let historical_kernel_excesses_for_mints = {
                        let c = self.chain.lock().unwrap();
                        c.kernel_excesses.clone()
                    };
                    let mut mint_ops = Vec::new();
                    for op in candidate_mint_ops {
                        let inputs_ok = op.fee_payment.inputs.iter()
                            .all(|i| utxos_snapshot.contains(&i.commitment) && !spent_this_block.contains(&i.commitment));
                        if !inputs_ok || asset_registry_snapshot.contains_key(&op.asset_id) {
                            continue;
                        }

                        if let (Some(collection_id), Some(phase_index)) = (&op.collection_id, op.phase_index) {
                            let Some(collection) = collection_registry_snapshot.get(collection_id) else { continue };
                            let Some(phase) = collection.phases.get(phase_index as usize) else { continue };
                            if now < phase.start_time || now >= phase.end_time {
                                continue;
                            }
                            if let Some(root) = phase.allowlist_merkle_root {
                                let (Some(proof), Some(leaf_index)) = (&op.allowlist_proof, op.allowlist_leaf_index) else { continue };
                                let leaf = crate::core::collections::allowlist_leaf(&op.owner_pubkey);
                                if !crate::core::merkle::verify_merkle_proof(leaf, proof, leaf_index as usize, root) {
                                    continue;
                                }
                            }
                            let owner_bytes = *op.owner_pubkey.as_point().compress().as_bytes();
                            let count_key = (collection_id.clone(), phase_index, owner_bytes);
                            let current_count = candidate_mint_counts.get(&count_key).copied().unwrap_or(0);
                            if current_count >= phase.per_wallet_limit {
                                continue;
                            }
                            let Some(required_excess) = op.required_kernel_excess else { continue };
                            let satisfied = historical_kernel_excesses_for_mints.contains(&required_excess)
                                || body_kernel_excesses.contains(&required_excess);
                            if !satisfied {
                                continue;
                            }
                            let Some(creator_sig) = &op.creator_signature else { continue };
                            let approval_msg = crate::core::assets::MintAssetOp::collection_approval_signing_message(&op.asset_id, collection_id, phase_index, &required_excess, &op.owner_pubkey);
                            if !creator_sig.verify(&approval_msg, &collection.creator_pubkey) {
                                continue;
                            }
                            candidate_mint_counts.insert(count_key, current_count + 1);
                        }

                        for i in &op.fee_payment.inputs {
                            spent_this_block.insert(i.commitment);
                        }
                        asset_registry_snapshot.insert(op.asset_id.clone(), crate::core::assets::AssetRecord {
                            asset_id: op.asset_id.clone(),
                            owner_pubkey: op.owner_pubkey,
                            metadata: op.metadata.clone(),
                            minted_at_block: next_height,
                            collection_id: op.collection_id.clone(),
                        });
                        mint_ops.push(op);
                    }
                    let candidate_transfer_asset_ops = {
                        let mut mp = self.mempool.lock().unwrap();
                        mp.take_transfer_asset_ops()
                    };
                    let original_asset_registry = {
                        let c = self.chain.lock().unwrap();
                        c.asset_registry.clone()
                    };
                    // Every kernel excess this candidate block would itself add,
                    // mirroring apply_linear_block's block_kernel_excesses - a
                    // conditional transfer (marketplace atomic swap) referencing
                    // a payment kernel bundled into THIS SAME block must not be
                    // filtered out here just because it hasn't landed historically
                    // yet (see core::assets::TransferAssetOp::required_kernel_excess).
                    let mut candidate_kernel_excesses: std::collections::HashSet<Commitment> =
                        tx.kernels.iter().map(|k| k.excess).collect();
                    for op in &mint_ops {
                        candidate_kernel_excesses.extend(op.fee_payment.kernels.iter().map(|k| k.excess));
                    }
                    for op in &name_ops {
                        candidate_kernel_excesses.extend(op.fee_payment.kernels.iter().map(|k| k.excess));
                    }
                    let historical_kernel_excesses = {
                        let c = self.chain.lock().unwrap();
                        c.kernel_excesses.clone()
                    };
                    let mut assets_touched: std::collections::HashSet<String> = mint_ops.iter().map(|op| op.asset_id.clone()).collect();
                    let mut transfer_asset_ops = Vec::new();
                    for op in candidate_transfer_asset_ops {
                        if assets_touched.contains(&op.asset_id) {
                            continue;
                        }
                        let Some(current) = original_asset_registry.get(&op.asset_id) else { continue };
                        let msg = crate::core::assets::TransferAssetOp::signing_message(&op.asset_id, &op.new_owner_pubkey, &op.required_kernel_excess, &op.required_royalty_kernel_excess);
                        if !op.signature.verify(&msg, &current.owner_pubkey) {
                            continue;
                        }
                        if let Some(required_excess) = op.required_kernel_excess {
                            let satisfied = historical_kernel_excesses.contains(&required_excess)
                                || candidate_kernel_excesses.contains(&required_excess);
                            if !satisfied {
                                continue;
                            }
                        }
                        // Only applies to an actual sale (required_kernel_excess
                        // is Some) - mirrors ChainState::apply_linear_block's
                        // own gate, see its doc comment for why an
                        // unconditional transfer must stay exempt.
                        if op.required_kernel_excess.is_some() {
                            if let Some(collection_id) = &current.collection_id {
                                if let Some(collection) = collection_registry_snapshot.get(collection_id) {
                                    if collection.royalty_bps > 0 {
                                        let Some(required_royalty) = op.required_royalty_kernel_excess else { continue };
                                        let satisfied = historical_kernel_excesses.contains(&required_royalty)
                                            || candidate_kernel_excesses.contains(&required_royalty);
                                        if !satisfied {
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                        assets_touched.insert(op.asset_id.clone());
                        asset_registry_snapshot.insert(op.asset_id.clone(), crate::core::assets::AssetRecord {
                            asset_id: op.asset_id.clone(),
                            owner_pubkey: op.new_owner_pubkey,
                            metadata: current.metadata.clone(),
                            minted_at_block: current.minted_at_block,
                            collection_id: current.collection_id.clone(),
                        });
                        transfer_asset_ops.push(op);
                    }

                    let asset_registry_root = crate::core::assets::compute_asset_registry_root(&asset_registry_snapshot);

                    println!("We are chosen proposer for block #{}! Proposing block with {} user transactions, {} name registrations, {} name transfers, {} asset mints, {} asset transfers...", next_height, tx.kernels.len(), name_ops.len(), transfer_ops.len(), mint_ops.len(), transfer_asset_ops.len());
                    
                    // 1. Calculate total fees and coinbase value
                    let total_fees: u64 = tx.kernels.iter().map(|k| k.fee).sum();
                    let coinbase_value = super::block::block_reward_at(next_height) + total_fees;

                    // 2. Derive this block's coinbase blinding from our own
                    // staking secret (private_key) instead of a random,
                    // immediately-discarded one - the old approach meant
                    // NOBODY, not even the proposer who earned it, could ever
                    // prove ownership of a block reward, permanently
                    // "burning" every block's minted coins. private_key is
                    // already a real secret this validator holds (see
                    // reveal_stake_blinding_hex - it's literally the blinding
                    // of whatever UTXO they staked), so no new key material
                    // is needed, just a deterministic derivation instead of
                    // Scalar::random.
                    let r_coinbase = crate::wallet::note::coinbase_blinding(&private_key, next_height);

                    // 3. Create coinbase output and range proof
                    let coinbase_commitment = Commitment::new(coinbase_value, r_coinbase);
                    let coinbase_proof = crate::crypto::range_proof::RangeProof::prove(coinbase_value, &r_coinbase);
                    let coinbase_note_key = crate::wallet::note::coinbase_note_key(&private_key);
                    let coinbase_note = crate::wallet::note::seal(&coinbase_note_key, next_height as u32, coinbase_value);
                    let coinbase_output = crate::core::transaction::Output {
                        commitment: coinbase_commitment,
                        proof: coinbase_proof,
                        note: coinbase_note,
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

                    let mut header = BlockHeader {
                        height: next_height,
                        prev_hash,
                        total_kernel_offset: Scalar::zero(),
                        nonce: 0,
                        timestamp: now,
                        validator_commitment: validator.commitment,
                        validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
                        name_registry_root,
                        chain_id: crate::core::genesis::CHAIN_ID,
                        asset_registry_root,
                        collection_registry_root,
                    };

                    // Sign the block header
                    let msg = header.hash();
                    header.validator_signature = Signature::sign(&msg, &private_key);

                    let block = Block {
                        header,
                        body: tx,
                        name_ops,
                        transfer_ops,
                        mint_ops,
                        transfer_asset_ops,
                        launch_collection_ops,
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
