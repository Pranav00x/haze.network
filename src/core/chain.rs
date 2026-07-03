use std::collections::{HashSet, HashMap};
use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use super::transaction::{TxKernel, Output};
use super::block::Block;
use super::registry::{NameRecord, compute_registry_root};
use super::compaction::BlockPruneMeta;
use serde::{Serialize, Deserialize};
use curve25519_dalek_ng::scalar::Scalar;
use bulletproofs::PedersenGens;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Validator {
    pub commitment: Commitment,
    pub value: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ChainState {
    /// The Unspent Transaction Output (UTXO) set.
    pub utxos: HashSet<Commitment>,
    /// All transaction kernels ever recorded on the chain
    pub kernels: Vec<TxKernel>,
    pub current_height: u64,
    pub last_block_hash: [u8; 32],
    /// Staking validators active on the network
    pub active_validators: Vec<Validator>,
    /// Block database
    pub blocks: HashMap<[u8; 32], Block>,
    /// Snapshots of active validators at each block height
    pub validator_snapshots: HashMap<u64, Vec<Validator>>,
    /// The Haze Naming Registry - name -> record, committed into consensus
    /// state via BlockHeader::name_registry_root (see core::registry).
    pub name_registry: HashMap<String, NameRecord>,
    /// Pre-transfer NameRecord for each name transferred at a given height -
    /// same purpose as validator_snapshots: apply_linear_block populates it,
    /// rollback_block consumes it to restore the exact prior record.
    pub transfer_snapshots: HashMap<u64, Vec<(String, NameRecord)>>,
    /// Per-block bookkeeping for horizon-based cut-through (see
    /// core::compaction::compact) - which blocks have had inputs/outputs
    /// physically removed, and how many. Purely a display aid (see
    /// api::explorer), never consulted by validation.
    pub prune_meta: HashMap<[u8; 32], BlockPruneMeta>,
}

/// Describes exactly what changed when a block was successfully applied, so storage can
/// persist just the delta instead of rewriting the whole chain state.
#[derive(Debug, Clone)]
pub struct AppliedDelta {
    pub block: Block,
    pub spent_commitments: Vec<Commitment>,
    pub new_outputs: Vec<Output>,
    pub new_kernels: Vec<TxKernel>,
    /// Height -> validator set snapshotted immediately before this block was applied.
    pub validator_snapshot: (u64, Vec<Validator>),
    /// Active validator set immediately after this block was applied.
    pub active_validators_after: Vec<Validator>,
    /// Names registered by this block (empty for most blocks).
    pub new_names: Vec<(String, NameRecord)>,
    /// Names transferred by this block, paired with their PRE-transfer
    /// record so a rollback can restore it exactly.
    pub transferred_names: Vec<(String, NameRecord)>,
    pub height: u64,
    pub tip_hash: [u8; 32],
}

/// Describes exactly what changed when the chaintip block was rolled back.
#[derive(Debug, Clone)]
pub struct RollbackDelta {
    pub un_consumed_outputs: Vec<Commitment>,
    pub restored_inputs: Vec<Commitment>,
    pub removed_kernel_excesses: Vec<Commitment>,
    /// Active validator set immediately after this rollback.
    pub active_validators_after: Vec<Validator>,
    /// Names un-registered by this rollback.
    pub removed_names: Vec<String>,
    pub new_height: u64,
    pub new_tip: [u8; 32],
}

/// Outcome of attempting to apply a new block to the chain state.
#[derive(Debug, Clone)]
pub enum ApplyResult {
    Rejected,
    Linear(AppliedDelta),
    Reorg { rollbacks: Vec<RollbackDelta>, applies: Vec<AppliedDelta> },
}

impl ApplyResult {
    pub fn is_applied(&self) -> bool {
        !matches!(self, ApplyResult::Rejected)
    }
}

impl ChainState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_validator(&mut self, commitment: Commitment, value: u64, blinding: Scalar) -> bool {
        if Commitment::new(value, blinding) != commitment {
            return false;
        }
        if !self.utxos.contains(&commitment) {
            return false;
        }
        if let Some(pos) = self.active_validators.iter().position(|v| v.commitment == commitment) {
            if self.active_validators[pos].value == value {
                return false;
            }
            self.active_validators[pos].value = value;
        } else {
            self.active_validators.push(Validator { commitment, value });
        }
        true
    }

    /// Adopts a validator entry learned from a peer (via GetValidators/
    /// ValidatorsList), rather than a fresh registration - so it only checks
    /// that the commitment matches a UTXO we actually have (proving it's a
    /// real, still-unspent output), without requiring the blinding factor
    /// again. The peer already proved ownership once when it was first
    /// registered; this just lets a syncing/reconnecting node catch up on
    /// state that isn't otherwise part of block history.
    pub fn adopt_validator(&mut self, commitment: Commitment, value: u64) -> bool {
        if !self.utxos.contains(&commitment) {
            return false;
        }
        if let Some(pos) = self.active_validators.iter().position(|v| v.commitment == commitment) {
            if self.active_validators[pos].value == value {
                return false;
            }
            self.active_validators[pos].value = value;
        } else {
            self.active_validators.push(Validator { commitment, value });
        }
        true
    }

    /// Deterministically selects the block proposer for a given height and previous hash.
    pub fn select_proposer(&self, height: u64, prev_hash: [u8; 32]) -> Commitment {
        self.proposer_priority_order(height, prev_hash)[0]
    }

    /// The full weighted priority order of active validators for a given
    /// height: index 0 is the primary proposer (identical to the historical
    /// select_proposer result), index 1 is the first fallback a client
    /// should try if the primary hasn't produced a block after a timeout,
    /// and so on. Consensus (apply_linear_block) no longer requires the
    /// signer to be exactly index 0 - this ordering is purely a liveness
    /// convention honest clients follow (see core::proposer), not something
    /// enforced at the protocol level.
    pub fn proposer_priority_order(&self, height: u64, prev_hash: [u8; 32]) -> Vec<Commitment> {
        if self.active_validators.is_empty() {
            // Default to genesis validator commitment
            let genesis_blinding = Scalar::from(42u64);
            return vec![Commitment::new(1_000_000, genesis_blinding)];
        }

        let mut pool: Vec<Validator> = self.active_validators.clone();
        let mut order = Vec::with_capacity(pool.len());

        for round in 0..pool.len() {
            let total_stake: u64 = pool.iter().map(|v| v.value).sum();
            let selected_index = if total_stake == 0 {
                0
            } else {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(&height.to_le_bytes());
                hasher.update(&prev_hash);
                // Round 0 intentionally matches the original select_proposer
                // hash exactly (no round byte) for backward compatibility;
                // later rounds mix in the round index so each fallback draw
                // is independent of the others.
                if round > 0 {
                    hasher.update(&(round as u64).to_le_bytes());
                }
                let hash = hasher.finalize();

                let mut seed_bytes = [0u8; 8];
                seed_bytes.copy_from_slice(&hash[0..8]);
                let seed = u64::from_le_bytes(seed_bytes);

                let mut target = seed % total_stake;
                let mut index = 0;
                for (i, validator) in pool.iter().enumerate() {
                    if target < validator.value {
                        index = i;
                        break;
                    }
                    target -= validator.value;
                }
                index
            };

            order.push(pool[selected_index].commitment);
            pool.remove(selected_index);
        }

        order
    }

    /// Attempts to apply a new block to the chain state.
    /// Triggers a reorganization if the block is on a heavier/taller fork.
    pub fn apply_block(&mut self, block: &Block) -> ApplyResult {
        let block_hash = block.header.hash();

        // Store block in block database first
        self.blocks.insert(block_hash, block.clone());

        // Case 1: Simple linear application on top of current chaintip (or genesis block on fresh tip)
        let is_linear = (block.header.height == self.current_height + 1 && block.header.prev_hash == self.last_block_hash)
            || (block.header.height == 0 && self.current_height == 0 && self.last_block_hash == [0u8; 32]);

        if is_linear {
            return match self.apply_linear_block(block) {
                Some(delta) => ApplyResult::Linear(delta),
                None => ApplyResult::Rejected,
            };
        }

        // Case 2: Block is on a fork
        // Trigger reorg only if the fork block is taller/heavier than our current tip
        if block.header.height > self.current_height {
            if let Some((rollback_hashes, fork_blocks)) = self.find_reorg_path(block) {
                println!("ChainState: Reorganization triggered! Rolling back {} blocks, applying {} fork blocks", rollback_hashes.len(), fork_blocks.len());
                let mut sandbox = self.clone();

                // Roll back current tip to common ancestor
                let mut rollback_deltas = Vec::new();
                for _hash in &rollback_hashes {
                    match sandbox.rollback_block() {
                        Some(delta) => rollback_deltas.push(delta),
                        None => return ApplyResult::Rejected,
                    }
                }

                // Apply the new fork blocks
                let mut applied_deltas = Vec::new();
                for fb in &fork_blocks {
                    match sandbox.apply_linear_block(fb) {
                        Some(delta) => applied_deltas.push(delta),
                        None => return ApplyResult::Rejected,
                    }
                }

                // Reorg successful, commit changes
                *self = sandbox;
                return ApplyResult::Reorg { rollbacks: rollback_deltas, applies: applied_deltas };
            }
        }

        // Otherwise, we just store it in self.blocks (done above) and reject
        ApplyResult::Rejected
    }

    /// Directly applies a block on top of the current tip.
    pub fn apply_linear_block(&mut self, block: &Block) -> Option<AppliedDelta> {
        // 1. Verify height and prev_hash connectivity
        if block.header.height != self.current_height + 1 {
            // Special case for Genesis Block (height 0) on fresh state
            if !(block.header.height == 0 && self.current_height == 0 && self.last_block_hash == [0u8; 32]) {
                return None;
            }
        } else if block.header.prev_hash != self.last_block_hash {
            return None;
        }

        // 2. Verify Proof of Stake block proposer signature. Any currently
        // active, registered validator may sign any height's block - see
        // proposer_priority_order for how honest clients rank who tries
        // first (a liveness convention enforced client-side, not a
        // consensus rule). Previously exactly one computed winner
        // (select_proposer) was ever authorized to sign a given height; if
        // that validator went offline, the chain stalled on that height
        // forever (confirmed via a live 3-node test). Before any validator
        // has ever registered, only the well-known genesis-bootstrap
        // commitment may propose, matching Proposer::start_proposing's own
        // fallback.
        let proposer_commitment = block.header.validator_commitment;
        let genesis_default = Commitment::new(1_000_000, Scalar::from(42u64));

        let stake_value = if self.active_validators.is_empty() {
            if proposer_commitment != genesis_default {
                return None;
            }
            1_000_000
        } else {
            match self.active_validators.iter().find(|v| v.commitment == proposer_commitment) {
                Some(v) => v.value,
                None => return None,
            }
        };

        let gens = PedersenGens::default();
        let p_sig_point = proposer_commitment.as_point() - Scalar::from(stake_value) * gens.B;
        let p_sig_commitment = Commitment(p_sig_point);

        let mut header_copy = block.header.clone();
        header_copy.validator_signature = Signature { s: Scalar::zero(), e: Scalar::zero() };
        let msg = header_copy.hash();

        if block.header.height > 0 && !block.header.validator_signature.verify(&msg, &p_sig_commitment) {
            return None;
        }

        // 3. Verify the block's internal cryptography
        let block_reward = if block.header.height == 0 {
            super::genesis::GENESIS_TOTAL_MINTED
        } else {
            super::block::block_reward_at(block.header.height)
        };

        if !block.body.validate_with_reward(block_reward) {
            return None;
        }

        // 3b. Reject blocks from a different network outright - a mismatched
        // chain_id means this block was never meant for this chain at all
        // (see core::genesis::CHAIN_ID).
        if block.header.chain_id != super::genesis::CHAIN_ID {
            return None;
        }

        // 3c. Reject any block that spends a team/investor vesting tranche
        // before its unlock height - the timelock enforcing the 6-month
        // cliff + 2-year quarterly vesting schedule (see core::vesting).
        if super::vesting::spends_locked_output_early(&block.body.inputs, block.header.height) {
            return None;
        }

        // 4. Ensure all inputs exist in our UTXO set (no double spends, no fake inputs)
        // Skip input checks for genesis block (since it has no inputs)
        if block.header.height > 0 {
            for input in &block.body.inputs {
                if !self.utxos.contains(&input.commitment) {
                    return None;
                }
            }
        }

        // 4b. Validate name registrations (see core::registry). Each op is checked
        // standalone (name rules, ownership signature, its own fee-payment balance),
        // plus chain-state checks that can only happen here: the name isn't already
        // taken (by prior blocks or an earlier op in this same block), it isn't
        // registered twice within this block, and its fee-payment inputs are real,
        // currently-unspent UTXOs not already consumed by body or an earlier op.
        let mut spent_this_block: HashSet<Commitment> = block.body.inputs.iter().map(|i| i.commitment).collect();
        let mut names_this_block: HashSet<&str> = HashSet::new();
        let mut candidate_registry = self.name_registry.clone();
        for op in &block.name_ops {
            if op.validate_standalone().is_err() {
                return None;
            }
            if self.name_registry.contains_key(&op.name) || !names_this_block.insert(op.name.as_str()) {
                return None;
            }
            for input in &op.fee_payment.inputs {
                if !self.utxos.contains(&input.commitment) || spent_this_block.contains(&input.commitment) {
                    return None;
                }
                spent_this_block.insert(input.commitment);
            }
            candidate_registry.insert(op.name.clone(), NameRecord {
                name: op.name.clone(),
                owner_pubkey: op.owner_pubkey,
                resolves_to: op.resolves_to,
                registered_at_block: block.header.height,
            });
        }

        // 4c. Validate name transfers. Each must target an already-registered
        // name (from a prior block - not one freshly registered earlier in
        // this same block, which names_this_block also guards against) and
        // be signed by that name's CURRENT owner.
        for op in &block.transfer_ops {
            if !names_this_block.insert(op.name.as_str()) {
                return None;
            }
            let current = match self.name_registry.get(&op.name) {
                Some(r) => r,
                None => return None,
            };
            let msg = super::registry::TransferNameOp::signing_message(&op.name, &op.new_owner_pubkey, &op.new_resolves_to);
            if !op.signature.verify(&msg, &current.owner_pubkey) {
                return None;
            }
            candidate_registry.insert(op.name.clone(), NameRecord {
                name: op.name.clone(),
                owner_pubkey: op.new_owner_pubkey,
                resolves_to: op.new_resolves_to,
                registered_at_block: current.registered_at_block,
            });
        }

        if compute_registry_root(&candidate_registry) != block.header.name_registry_root {
            return None;
        }

        // 5. Save the snapshot of active validators BEFORE applying state transitions
        let validator_snapshot = (block.header.height, self.active_validators.clone());
        self.validator_snapshots.insert(validator_snapshot.0, validator_snapshot.1.clone());

        // 6. Remove spent inputs from the UTXO set and validators
        let mut spent_commitments = Vec::new();
        if block.header.height > 0 {
            for input in &block.body.inputs {
                self.utxos.remove(&input.commitment);
                self.active_validators.retain(|val| val.commitment != input.commitment);
                spent_commitments.push(input.commitment);
            }
        }

        // 7. Add new outputs to the UTXO set
        for output in &block.body.outputs {
            self.utxos.insert(output.commitment);
        }

        // 7b. Apply each name op's fee-payment transaction (spend its inputs, add its
        // change outputs) and commit the registry entry itself.
        let mut new_names = Vec::new();
        for op in &block.name_ops {
            for input in &op.fee_payment.inputs {
                self.utxos.remove(&input.commitment);
                spent_commitments.push(input.commitment);
            }
            for output in &op.fee_payment.outputs {
                self.utxos.insert(output.commitment);
            }
            let record = candidate_registry[&op.name].clone();
            self.name_registry.insert(op.name.clone(), record.clone());
            new_names.push((op.name.clone(), record));
        }

        // 7c. Apply transfers, remembering the pre-transfer record so a
        // rollback can restore it exactly.
        let mut transferred_names = Vec::new();
        for op in &block.transfer_ops {
            let old_record = self.name_registry[&op.name].clone();
            let new_record = candidate_registry[&op.name].clone();
            self.name_registry.insert(op.name.clone(), new_record);
            transferred_names.push((op.name.clone(), old_record));
        }
        if !transferred_names.is_empty() {
            self.transfer_snapshots.insert(block.header.height, transferred_names.clone());
        }

        // 8. Save the kernels forever
        for kernel in &block.body.kernels {
            self.kernels.push(kernel.clone());
        }
        for op in &block.name_ops {
            for kernel in &op.fee_payment.kernels {
                self.kernels.push(kernel.clone());
            }
        }

        self.current_height = block.header.height;
        self.last_block_hash = block.header.hash();
        self.blocks.insert(self.last_block_hash, block.clone());

        Some(AppliedDelta {
            block: block.clone(),
            spent_commitments,
            new_outputs: block.body.outputs.clone(),
            new_kernels: block.body.kernels.clone(),
            new_names,
            transferred_names,
            validator_snapshot,
            active_validators_after: self.active_validators.clone(),
            height: self.current_height,
            tip_hash: self.last_block_hash,
        })
    }

    /// Rolls back the chaintip block by one height.
    /// Returns the delta if successful, None if there are no blocks to roll back.
    pub fn rollback_block(&mut self) -> Option<RollbackDelta> {
        if self.current_height == 0 {
            return None;
        }

        let tip_hash = self.last_block_hash;
        let tip_block = match self.blocks.get(&tip_hash) {
            Some(b) => b.clone(),
            None => return None,
        };

        // 1. Remove output commitments from UTXO set
        let mut un_consumed_outputs = Vec::new();
        for output in &tip_block.body.outputs {
            self.utxos.remove(&output.commitment);
            un_consumed_outputs.push(output.commitment);
        }

        // 2. Restore spent input commitments back to UTXO set
        let mut restored_inputs = Vec::new();
        for input in &tip_block.body.inputs {
            self.utxos.insert(input.commitment);
            restored_inputs.push(input.commitment);
        }

        // 3. Remove block kernels
        let mut removed_kernel_excesses = Vec::new();
        for kernel in &tip_block.body.kernels {
            self.kernels.retain(|k| k.excess != kernel.excess);
            removed_kernel_excesses.push(kernel.excess);
        }

        // 3b. Revert each name op's fee-payment transaction and registry entry.
        let mut removed_names = Vec::new();
        for op in &tip_block.name_ops {
            for output in &op.fee_payment.outputs {
                self.utxos.remove(&output.commitment);
            }
            for input in &op.fee_payment.inputs {
                self.utxos.insert(input.commitment);
                restored_inputs.push(input.commitment);
            }
            for kernel in &op.fee_payment.kernels {
                self.kernels.retain(|k| k.excess != kernel.excess);
            }
            self.name_registry.remove(&op.name);
            removed_names.push(op.name.clone());
        }

        // 3c. Revert each transfer back to its pre-transfer record.
        if let Some(pre_transfer) = self.transfer_snapshots.remove(&self.current_height) {
            for (name, old_record) in pre_transfer {
                self.name_registry.insert(name, old_record);
            }
        }

        // 4. Restore active validator registry snapshot from height H-1
        let prev_height = self.current_height - 1;
        self.active_validators = self.validator_snapshots.get(&prev_height).cloned().unwrap_or_default();

        // 5. Clean up snapshot at height H
        self.validator_snapshots.remove(&self.current_height);

        // 6. Update tip metadata
        self.current_height = prev_height;
        self.last_block_hash = tip_block.header.prev_hash;

        Some(RollbackDelta {
            un_consumed_outputs,
            restored_inputs,
            removed_kernel_excesses,
            active_validators_after: self.active_validators.clone(),
            removed_names,
            new_height: self.current_height,
            new_tip: self.last_block_hash,
        })
    }

    /// The oldest height this node can still guarantee FULL (unpruned)
    /// block data for. A syncing peer requesting history older than this
    /// would receive blocks missing some inputs/outputs that horizon-based
    /// compaction has already removed - fine for this node's own already-
    /// validated state, but a fresh peer re-validating each block's balance
    /// equation from scratch would fail on them (see core::compaction's
    /// module docs for why). Nodes that have never compacted anything just
    /// report height 0 (fully archival).
    pub fn earliest_full_height(&self) -> u64 {
        if self.prune_meta.is_empty() {
            return 0;
        }
        self.prune_meta.keys()
            .filter_map(|hash| self.blocks.get(hash))
            .map(|b| b.header.height)
            .max()
            .map(|h| h + 1)
            .unwrap_or(0)
    }

    /// Returns up to `limit` blocks starting at `from_height` from the active chain,
    /// walking backward from the tip via prev_hash. `has_more` is true if there were
    /// additional blocks beyond `limit`.
    pub fn get_blocks_from(&self, from_height: u64, limit: usize) -> (Vec<Block>, bool) {
        let mut collected: Vec<Block> = Vec::new();
        let mut current_hash = self.last_block_hash;

        loop {
            let block = match self.blocks.get(&current_hash) {
                Some(b) => b.clone(),
                None => break,
            };
            if block.header.height < from_height {
                break;
            }
            let prev = block.header.prev_hash;
            let height = block.header.height;
            collected.push(block);
            if height == 0 {
                break;
            }
            current_hash = prev;
        }

        collected.reverse();

        // `collected` is ascending by height starting at `from_height`. Keep the
        // oldest `limit` blocks (i.e. the ones actually starting at `from_height`) -
        // truncating from the end, not the start.
        let has_more = collected.len() > limit;
        collected.truncate(limit);
        (collected, has_more)
    }

    /// Finds a reorganization path from current active tip to a new block.
    fn find_reorg_path(&self, new_block: &Block) -> Option<(Vec<[u8; 32]>, Vec<Block>)> {
        let mut fork_blocks = Vec::new();
        fork_blocks.push(new_block.clone());

        let mut current_hash = new_block.header.prev_hash;

        while current_hash != [0u8; 32] {
            // Check if current_hash lies on our active chain chaintip history
            let mut is_on_active_chain = false;
            let mut trace = self.last_block_hash;
            while trace != [0u8; 32] {
                if trace == current_hash {
                    is_on_active_chain = true;
                    break;
                }
                if let Some(b) = self.blocks.get(&trace) {
                    trace = b.header.prev_hash;
                } else {
                    break;
                }
            }

            if is_on_active_chain {
                // Found common ancestor!
                let mut rollback_hashes = Vec::new();
                let mut active_trace = self.last_block_hash;
                while active_trace != current_hash && active_trace != [0u8; 32] {
                    rollback_hashes.push(active_trace);
                    if let Some(b) = self.blocks.get(&active_trace) {
                        active_trace = b.header.prev_hash;
                    } else {
                        break;
                    }
                }
                fork_blocks.reverse();
                return Some((rollback_hashes, fork_blocks));
            }

            // Walk back via prev_hash
            if let Some(parent_b) = self.blocks.get(&current_hash) {
                fork_blocks.push(parent_b.clone());
                current_hash = parent_b.header.prev_hash;
            } else {
                return None; // Orphan fork
            }
        }

        None
    }
}
