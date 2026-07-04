//! Horizon-based cut-through, matching Grin's approach to bounding chain
//! size over time: once an output has been spent AND the block that spent
//! it is deep enough that a reorg could never realistically reach it again,
//! both the output (in the block that created it) and the input (in the
//! block that spent it) are pure historical witness data - nobody needs them
//! to determine current chain state, only kernels (the permanent proof a
//! transaction balanced) and the current UTXO set matter for that.
//!
//! This is safe here specifically because `BlockHeader::hash()` (see
//! core::block) only ever hashes header fields - height, prev_hash, kernel
//! offset, nonce, timestamp, validator commitment/signature, and
//! name_registry_root - never `block.body.inputs`/`outputs`. Removing
//! specific input/output entries from an already-applied block's stored
//! body cannot change that block's hash, any descendant's hash, the current
//! UTXO set, or current_height: compact() only ever touches
//! `ChainState.blocks`' body vectors (plus the snapshot maps, which are pure
//! reorg-rollback aids), never `utxos`, `kernels`, `current_height`, or
//! `last_block_hash`.
//!
//! Scope: this compacts a node's OWN storage for outputs it already
//! validated live. It deliberately does NOT attempt to let a brand-new node
//! fully re-derive chain state from scratch through a pruned peer's P2P
//! history - Transaction::validate_with_reward re-checks each block's own
//! balance equation at apply time (including during sync), and a pruned
//! block's kernel still encodes the original balance math for its now-absent
//! output/input, so a fresh node re-validating that stripped block from zero
//! would fail. Real Grin solves this with aggregate/kernel-offset validation
//! instead of strict per-block balance checking - a bigger, separate change.
//! For now, p2p::server declines to serve GetBlocks ranges reaching into
//! pruned territory rather than silently handing out blocks a fresh peer
//! can't validate.
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};

use crate::crypto::pedersen::Commitment;
use super::chain::ChainState;

/// Default horizon, in blocks: anything older than this from the current
/// tip is eligible for compaction. Tunable, and deliberately just a default
/// - compact() takes the horizon as a parameter (both for testability and
/// because, unlike NAME_REGISTRATION_FEE elsewhere, this has no consensus
/// meaning: two nodes running different horizons still agree on chain tip
/// and UTXO set, they just retain different amounts of no-longer-needed
/// history).
pub const CUT_THROUGH_HORIZON: u64 = 1000;

/// Per-block bookkeeping for compact() - purely a display aid (see
/// api::explorer), never consulted by consensus. Tracks how many
/// inputs/outputs were physically removed from this block's stored body, so
/// the explorer can show the true original counts instead of silently
/// looking like the block always had fewer.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BlockPruneMeta {
    pub pruned_inputs: u32,
    pub pruned_outputs: u32,
}

/// Summarizes what a single compact() pass did - `touched_blocks` tells the
/// storage layer exactly which blocks need re-persisting (only those, not
/// the whole `blocks` tree).
#[derive(Debug, Default, Clone)]
pub struct CompactionReport {
    pub touched_blocks: Vec<[u8; 32]>,
    pub outputs_pruned: usize,
    pub inputs_pruned: usize,
    pub validator_snapshots_pruned: usize,
    pub transfer_snapshots_pruned: usize,
}

/// Runs one horizon-based cut-through pass over `chain`. Pure and
/// synchronous - never touches storage itself (see Compactor below for the
/// periodic/persisted version) - so it's trivially testable against a bare
/// ChainState.
pub fn compact(chain: &mut ChainState, horizon: u64) -> CompactionReport {
    let mut report = CompactionReport::default();
    let horizon_height = chain.current_height.saturating_sub(horizon);

    // Snapshots are pure rollback aids, not permanent history - a reorg
    // reaching back past the horizon is the same "can't happen" assumption
    // that makes pruning safe at all, so anything older is dead weight.
    let before = chain.validator_snapshots.len();
    chain.validator_snapshots.retain(|h, _| *h >= horizon_height);
    report.validator_snapshots_pruned = before - chain.validator_snapshots.len();

    let before = chain.transfer_snapshots.len();
    chain.transfer_snapshots.retain(|h, _| *h >= horizon_height);
    report.transfer_snapshots_pruned = before - chain.transfer_snapshots.len();

    // ---- Phase 1 (read-only): find every spent, horizon-deep create/spend pair ----

    // commitment -> (block that spent it, that block's height)
    let mut spent_in: HashMap<Commitment, ([u8; 32], u64)> = HashMap::new();
    for (hash, block) in chain.blocks.iter() {
        for input in &block.body.inputs {
            spent_in.insert(input.commitment, (*hash, block.header.height));
        }
        for op in &block.name_ops {
            for input in &op.fee_payment.inputs {
                spent_in.insert(input.commitment, (*hash, block.header.height));
            }
        }
    }

    // creating block hash -> commitments to drop from its stored outputs
    let mut prune_outputs: HashMap<[u8; 32], HashSet<Commitment>> = HashMap::new();
    // spending block hash -> commitments to drop from its stored inputs
    let mut prune_inputs: HashMap<[u8; 32], HashSet<Commitment>> = HashMap::new();

    for (hash, block) in chain.blocks.iter() {
        if block.header.height >= horizon_height {
            continue; // not old enough to even consider - leaves the whole recent window untouched
        }

        let mut consider = |commitment: Commitment| {
            if chain.utxos.contains(&commitment) {
                return; // still unspent - never prune, regardless of age
            }
            let Some((spend_hash, spend_height)) = spent_in.get(&commitment) else {
                return; // no record of it being spent at all - leave it alone
            };
            if spend_height + horizon > chain.current_height {
                return; // spent, but not deep enough yet - could still theoretically be reorged
            }
            prune_outputs.entry(*hash).or_default().insert(commitment);
            prune_inputs.entry(*spend_hash).or_default().insert(commitment);
        };

        for output in &block.body.outputs {
            consider(output.commitment);
        }
        for op in &block.name_ops {
            for output in &op.fee_payment.outputs {
                consider(output.commitment);
            }
        }
    }

    // ---- Phase 2 (mutate): actually remove the identified entries ----

    let mut touched: HashSet<[u8; 32]> = HashSet::new();
    touched.extend(prune_outputs.keys().copied());
    touched.extend(prune_inputs.keys().copied());

    for hash in &touched {
        let Some(block) = chain.blocks.get_mut(hash) else { continue };
        let mut meta = chain.prune_meta.remove(hash).unwrap_or_default();

        if let Some(commitments) = prune_outputs.get(hash) {
            let before = block.body.outputs.len();
            block.body.outputs.retain(|o| !commitments.contains(&o.commitment));
            meta.pruned_outputs += (before - block.body.outputs.len()) as u32;
            for op in &mut block.name_ops {
                let before = op.fee_payment.outputs.len();
                op.fee_payment.outputs.retain(|o| !commitments.contains(&o.commitment));
                meta.pruned_outputs += (before - op.fee_payment.outputs.len()) as u32;
            }
            report.outputs_pruned += commitments.len();
        }

        if let Some(commitments) = prune_inputs.get(hash) {
            let before = block.body.inputs.len();
            block.body.inputs.retain(|i| !commitments.contains(&i.commitment));
            meta.pruned_inputs += (before - block.body.inputs.len()) as u32;
            for op in &mut block.name_ops {
                let before = op.fee_payment.inputs.len();
                op.fee_payment.inputs.retain(|i| !commitments.contains(&i.commitment));
                meta.pruned_inputs += (before - op.fee_payment.inputs.len()) as u32;
            }
            report.inputs_pruned += commitments.len();
        }

        chain.prune_meta.insert(*hash, meta);
    }

    report.touched_blocks = touched.into_iter().collect();
    report
}

#[cfg(feature = "native")]
mod background {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::sleep;

    use super::{compact, CUT_THROUGH_HORIZON};
    use crate::core::chain::ChainState;
    use crate::core::storage::Storage;

    /// How often to check whether it's worth running a compaction pass -
    /// same "poll on a timer, mirror the proposer's pattern" approach as
    /// core::proposer::Proposer, just far less frequent since compaction is
    /// cheap to skip and not worth running constantly over a large chain.
    const CHECK_INTERVAL: Duration = Duration::from_secs(600);

    pub struct Compactor {
        chain: Arc<Mutex<ChainState>>,
        storage: Arc<Storage>,
        last_run_height: Mutex<u64>,
    }

    impl Compactor {
        pub fn new(chain: Arc<Mutex<ChainState>>, storage: Arc<Storage>) -> Self {
            Self { chain, storage, last_run_height: Mutex::new(0) }
        }

        /// Runs forever, checking periodically whether enough new height has
        /// accumulated since the last pass to make another one worthwhile -
        /// never blocks block validation/proposing, since it only ever takes
        /// the same chain Mutex any other request briefly takes, same as the
        /// API server or P2P handlers do.
        pub async fn run_periodic(&self) {
            println!("Compactor started (horizon = {} blocks).", CUT_THROUGH_HORIZON);
            loop {
                sleep(CHECK_INTERVAL).await;

                let current_height = { self.chain.lock().unwrap().current_height };
                let last_run = { *self.last_run_height.lock().unwrap() };
                if current_height < CUT_THROUGH_HORIZON || current_height - last_run < CUT_THROUGH_HORIZON / 10 {
                    continue; // not enough new prunable depth to bother yet
                }

                let report = {
                    let mut chain = self.chain.lock().unwrap();
                    compact(&mut chain, CUT_THROUGH_HORIZON)
                };

                if !report.touched_blocks.is_empty() {
                    println!(
                        "Compactor: pruned {} outputs / {} inputs across {} blocks (validator snapshots: {}, transfer snapshots: {}).",
                        report.outputs_pruned, report.inputs_pruned, report.touched_blocks.len(),
                        report.validator_snapshots_pruned, report.transfer_snapshots_pruned,
                    );
                    let chain = self.chain.lock().unwrap();
                    if let Err(e) = self.storage.persist_compaction(&chain, &report.touched_blocks) {
                        println!("Warning: Failed to persist compaction: {}", e);
                    }
                }

                *self.last_run_height.lock().unwrap() = current_height;
            }
        }
    }
}

#[cfg(feature = "native")]
pub use background::Compactor;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::block::{Block, BlockHeader};
    use crate::core::transaction::{Input, Output, Transaction, TxKernel};
    use crate::crypto::range_proof::RangeProof;
    use crate::crypto::schnorr::Signature;
    use curve25519_dalek_ng::scalar::Scalar;
    use rand::rngs::OsRng;

    const TEST_HORIZON: u64 = 20;

    fn empty_registry_root() -> [u8; 32] {
        crate::core::registry::compute_registry_root(&std::collections::HashMap::new())
    }

    /// Builds a real, correctly-signed one-input-one-output block spending
    /// `spend` (if any) and creating a fresh output of `value` - private_key
    /// = 42 matches the well-known devnet genesis validator ChainState::
    /// select_proposer defaults to whenever active_validators is empty
    /// (exactly the tests' setup, a fresh ChainState::new()).
    fn make_block(height: u64, prev_hash: [u8; 32], spend: Option<(Commitment, Scalar)>, new_value: u64) -> (Block, Scalar, Commitment) {
        let private_key = Scalar::from(42u64);
        let mut rng = OsRng;
        let r_out = Scalar::random(&mut rng);
        let out_commitment = Commitment::new(new_value, r_out);
        let out_proof = RangeProof::prove(new_value, &r_out);
        let output = Output { commitment: out_commitment, proof: out_proof, note: vec![] };

        let (inputs, excess_r) = match spend {
            Some((commitment, r_in)) => (vec![Input { commitment }], r_in - r_out),
            None => (vec![], Scalar::zero() - r_out),
        };

        let kernel = TxKernel {
            excess: Commitment::new(0, excess_r),
            fee: 0,
            signature: Signature::sign(&0u64.to_le_bytes(), &excess_r),
        };

        let mut header = BlockHeader {
            height,
            prev_hash,
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: empty_registry_root(),
            chain_id: crate::core::genesis::CHAIN_ID,
            asset_registry_root: crate::core::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block {
            header,
            body: Transaction { inputs, outputs: vec![output], kernels: vec![kernel] },
            name_ops: vec![],
            transfer_ops: vec![],
            mint_ops: vec![],
            transfer_asset_ops: vec![],
        };

        (block, r_out, out_commitment)
    }

    /// Builds a chain with real prunable history: the real genesis block
    /// (its special GENESIS_TOTAL_MINTED reward doesn't match an arbitrary
    /// test value, so it's reused as-is rather than faked), then spends its
    /// well-known validator-stake output (blinding=42, the one genesis
    /// output intentionally left public - see core::genesis) at height 1,
    /// followed by enough further single-input-single-output blocks (each
    /// spending the previous one's output and growing the value by the
    /// per-height block reward, matching the per-block reward
    /// apply_linear_block's balance check expects at any height > 0) to
    /// push that height-1 spend well past TEST_HORIZON.
    fn build_test_chain() -> ChainState {
        use crate::core::genesis::genesis_block;
        use crate::core::block::block_reward_at;

        let mut chain = ChainState::new();
        let genesis = genesis_block();
        assert!(chain.apply_block(&genesis).is_applied());

        let mut prev_hash = genesis.header.hash();
        let mut r_prev = Scalar::from(42u64);
        let mut c_prev = Commitment::new(1_000_000, r_prev);
        let mut value = 1_000_000u64;
        for h in 1..=(TEST_HORIZON + 10) {
            let next_value = value + block_reward_at(h);
            let (block, r_next, c_next) = make_block(h, prev_hash, Some((c_prev, r_prev)), next_value);
            assert!(chain.apply_block(&block).is_applied(), "block at height {} failed to apply", h);
            prev_hash = block.header.hash();
            r_prev = r_next;
            c_prev = c_next;
            value = next_value;
        }

        chain
    }

    #[test]
    fn compact_does_not_change_tip_hash_or_utxo_set() {
        let original = build_test_chain();
        let mut compacted = original.clone();

        let report = compact(&mut compacted, TEST_HORIZON);

        assert_eq!(original.current_height, compacted.current_height);
        assert_eq!(original.last_block_hash, compacted.last_block_hash);
        assert_eq!(original.utxos, compacted.utxos);
        assert_eq!(original.kernels.len(), compacted.kernels.len());

        // And compaction actually did something observable.
        assert!(report.outputs_pruned > 0, "expected at least one prunable output");
        assert!(report.inputs_pruned > 0, "expected at least one prunable input");
        assert!(!report.touched_blocks.is_empty());

        let touched_hash = report.touched_blocks[0];
        let touched_block = compacted.blocks.get(&touched_hash).unwrap();
        let original_block = original.blocks.get(&touched_hash).unwrap();
        assert!(
            touched_block.body.outputs.len() + touched_block.body.inputs.len()
                < original_block.body.outputs.len() + original_block.body.inputs.len()
        );
    }

    #[test]
    fn compact_never_touches_still_unspent_outputs() {
        let mut chain = build_test_chain();
        // The final output in the chain is still unspent (it's the current UTXO).
        let tip_output_commitment = *chain.utxos.iter().next().unwrap();

        compact(&mut chain, TEST_HORIZON);

        assert!(chain.utxos.contains(&tip_output_commitment));
        let still_present = chain.blocks.values()
            .any(|b| b.body.outputs.iter().any(|o| o.commitment == tip_output_commitment));
        assert!(still_present, "an unspent output must never be pruned from storage");
    }

    #[test]
    fn compact_leaves_recent_window_fully_intact() {
        let mut chain = build_test_chain();
        let before_recent = chain.blocks.get(&chain.last_block_hash).unwrap().clone();

        compact(&mut chain, TEST_HORIZON);

        let after_recent = chain.blocks.get(&chain.last_block_hash).unwrap();
        assert_eq!(before_recent.body.inputs.len(), after_recent.body.inputs.len());
        assert_eq!(before_recent.body.outputs.len(), after_recent.body.outputs.len());
    }

    #[test]
    fn compact_never_touches_kernels() {
        let mut chain = build_test_chain();
        let kernel_count_before = chain.kernels.len();

        compact(&mut chain, TEST_HORIZON);

        assert_eq!(chain.kernels.len(), kernel_count_before);
    }

    #[test]
    fn compact_records_prune_meta_for_explorer_display() {
        let mut chain = build_test_chain();
        let report = compact(&mut chain, TEST_HORIZON);

        let touched_hash = report.touched_blocks[0];
        let meta = chain.prune_meta.get(&touched_hash).expect("touched block must have prune_meta recorded");
        assert!(meta.pruned_outputs > 0 || meta.pruned_inputs > 0);
    }
}
