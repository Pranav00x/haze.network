use std::collections::{HashSet, HashMap};
use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use super::transaction::{TxKernel, Output};
use super::block::Block;
use super::registry::{NameRecord, compute_registry_root};
use super::assets::{AssetRecord, compute_asset_registry_root};
use super::collections::{CollectionRecord, compute_collection_registry_root, allowlist_leaf};
use super::merkle::verify_merkle_proof;
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
    /// Mirrors `kernels` 1:1 as a set of excesses, for O(1) existence checks
    /// (see TransferAssetOp::required_kernel_excess - the trustless
    /// atomic-swap primitive needs to answer "does a kernel with this
    /// excess exist" once per pending conditional transfer per candidate
    /// block, where a linear scan over `kernels` would become a real cost
    /// at scale). Not persisted directly (`#[serde(skip)]`) - rebuilt from
    /// `kernels` wherever chain state loads (see storage.rs, same place
    /// `kernels` itself gets rebuilt from the kernels sled tree).
    #[serde(skip)]
    pub kernel_excesses: HashSet<Commitment>,
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
    /// The Haze Asset Registry (NFTs) - asset_id -> record, committed into
    /// consensus state via BlockHeader::asset_registry_root (see
    /// core::assets). A separate namespace from name_registry.
    pub asset_registry: HashMap<String, AssetRecord>,
    /// Pre-transfer AssetRecord for each asset transferred at a given height -
    /// same purpose/pattern as transfer_snapshots.
    pub asset_transfer_snapshots: HashMap<u64, Vec<(String, AssetRecord)>>,
    /// Collection launches (NFT "drops" with scheduled multi-phase minting)
    /// - collection_id -> record, committed into consensus state via
    /// BlockHeader::collection_registry_root (see core::collections). A
    /// separate namespace from asset_registry - a collection groups mints,
    /// it isn't an asset itself.
    pub collection_registry: HashMap<String, CollectionRecord>,
    /// How many assets a given (collection_id, phase_index, owner pubkey)
    /// has minted so far - enforces MintPhase::per_wallet_limit. Keyed by
    /// compressed pubkey bytes rather than Commitment directly since this
    /// codebase's other maps key on Hash-friendly String/[u8;32] types (see
    /// core::collections::allowlist_leaf for the analogous reasoning on the
    /// Merkle side). Incrementally maintained (insert/increment on apply,
    /// decrement/remove on rollback) - same pattern as kernel_excesses.
    pub collection_mint_counts: HashMap<(String, u32, [u8; 32]), u32>,
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
    /// Assets minted by this block (empty for most blocks).
    pub new_assets: Vec<(String, AssetRecord)>,
    /// Assets transferred by this block, paired with their PRE-transfer
    /// record so a rollback can restore it exactly.
    pub transferred_assets: Vec<(String, AssetRecord)>,
    /// Collections launched by this block (empty for most blocks).
    pub new_collections: Vec<(String, CollectionRecord)>,
    /// Every (collection_id, phase_index, owner pubkey bytes) key this
    /// block incremented collection_mint_counts for - so a rollback can
    /// decrement exactly these, no more no less.
    pub collection_mints_this_block: Vec<(String, u32, [u8; 32])>,
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
    /// Assets un-minted by this rollback.
    pub removed_assets: Vec<String>,
    /// Collections un-launched by this rollback.
    pub removed_collections: Vec<String>,
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
            let genesis_blinding = crate::core::genesis::genesis_validator_blinding();
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
        let genesis_default = Commitment::new(1_000_000, crate::core::genesis::genesis_validator_blinding());

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

        // 4a-prep. Every kernel excess this candidate block itself would add,
        // were it applied - needed so a conditional asset transfer
        // (TransferAssetOp::required_kernel_excess) can reference a payment
        // kernel included in this SAME block, not just a historical one.
        // Without this, a payment and the transfer it unlocks could never
        // land in the same block, costing every swap an extra block of
        // latency for no real reason. Built once here since name/mint fee-
        // payment kernels aren't otherwise collected until step 8.
        let mut block_kernel_excesses: HashSet<Commitment> = block.body.kernels.iter().map(|k| k.excess).collect();
        for op in &block.name_ops {
            block_kernel_excesses.extend(op.fee_payment.kernels.iter().map(|k| k.excess));
        }
        for op in &block.mint_ops {
            block_kernel_excesses.extend(op.fee_payment.kernels.iter().map(|k| k.excess));
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

        // 4c-prep. Validate collection launches (see core::collections) -
        // same shape as 4b/4c but for collections rather than names: each op
        // is checked standalone (id rules, phase sanity, signature) plus the
        // one chain-state check that can only happen here - collection_id
        // isn't already taken (by a prior block or an earlier launch in this
        // same block). No fee-payment / UTXO involvement (LaunchCollectionOp
        // has none, by design - launching costs nothing beyond ordinary
        // block-inclusion). Built into a candidate_collection_registry so a
        // mint later in THIS SAME block can target a collection launched
        // earlier in it (mirrors block_kernel_excesses letting a payment and
        // the transfer/mint it unlocks land in one block).
        let mut collections_this_block: HashSet<&str> = HashSet::new();
        let mut candidate_collection_registry = self.collection_registry.clone();
        for op in &block.launch_collection_ops {
            if op.validate_standalone().is_err() {
                return None;
            }
            if self.collection_registry.contains_key(&op.collection_id) || !collections_this_block.insert(op.collection_id.as_str()) {
                return None;
            }
            candidate_collection_registry.insert(op.collection_id.clone(), CollectionRecord {
                collection_id: op.collection_id.clone(),
                creator_pubkey: op.creator_pubkey,
                name: op.name.clone(),
                symbol: op.symbol.clone(),
                metadata: op.metadata.clone(),
                phases: op.phases.clone(),
                launched_at_block: block.header.height,
                royalty_bps: op.royalty_bps,
            });
        }

        if compute_collection_registry_root(&candidate_collection_registry) != block.header.collection_registry_root {
            return None;
        }

        // 4d. Validate asset mints (see core::assets) - same shape as 4b,
        // separate namespace from names. `spent_this_block` is shared with
        // names/body so an asset mint's fee-payment can't double-spend an
        // input already claimed by any of them earlier in this same block.
        // A mint tagged with collection_id/phase_index additionally must:
        // target a real collection/phase (possibly launched in this same
        // block, see candidate_collection_registry above), fall within that
        // phase's [start_time, end_time) window per this block's own
        // timestamp, satisfy the phase's Merkle allowlist (if any), stay
        // under the phase's per-wallet mint limit, and - if conditioned on a
        // payment - only apply once that payment's kernel actually exists
        // (the same trustless atomic-swap primitive TransferAssetOp already
        // uses, applied to minting instead of transferring).
        let mut assets_this_block: HashSet<&str> = HashSet::new();
        let mut candidate_asset_registry = self.asset_registry.clone();
        let mut candidate_mint_counts = self.collection_mint_counts.clone();
        let mut collection_mints_this_block: Vec<(String, u32, [u8; 32])> = Vec::new();
        for op in &block.mint_ops {
            if op.validate_standalone().is_err() {
                return None;
            }
            if self.asset_registry.contains_key(&op.asset_id) || !assets_this_block.insert(op.asset_id.as_str()) {
                return None;
            }
            for input in &op.fee_payment.inputs {
                if !self.utxos.contains(&input.commitment) || spent_this_block.contains(&input.commitment) {
                    return None;
                }
                spent_this_block.insert(input.commitment);
            }

            if let (Some(collection_id), Some(phase_index)) = (&op.collection_id, op.phase_index) {
                let Some(collection) = candidate_collection_registry.get(collection_id) else { return None };
                let Some(phase) = collection.phases.get(phase_index as usize) else { return None };
                if block.header.timestamp < phase.start_time || block.header.timestamp >= phase.end_time {
                    return None;
                }
                if let Some(root) = phase.allowlist_merkle_root {
                    let (Some(proof), Some(leaf_index)) = (&op.allowlist_proof, op.allowlist_leaf_index) else { return None };
                    if !verify_merkle_proof(allowlist_leaf(&op.owner_pubkey), proof, leaf_index as usize, root) {
                        return None;
                    }
                }
                // A collection mint always requires both a payment condition
                // and the creator's explicit approval of it - regardless of
                // phase.price, since op.signature alone is signed by the
                // BUYER (see MintAssetOp::creator_signature's doc comment
                // for why the creator's own separate signature is the only
                // thing that actually ties this mint to a real payment).
                let (Some(required_excess), Some(creator_sig)) = (op.required_kernel_excess, &op.creator_signature) else { return None };
                let approval_msg = super::assets::MintAssetOp::collection_approval_signing_message(
                    &op.asset_id, collection_id, phase_index, &required_excess, &op.owner_pubkey,
                );
                if !creator_sig.verify(&approval_msg, &collection.creator_pubkey) {
                    return None;
                }
                let owner_bytes = *op.owner_pubkey.as_point().compress().as_bytes();
                let count_key = (collection_id.clone(), phase_index, owner_bytes);
                let current_count = candidate_mint_counts.get(&count_key).copied().unwrap_or(0);
                if current_count >= phase.per_wallet_limit {
                    return None;
                }
                candidate_mint_counts.insert(count_key.clone(), current_count + 1);
                collection_mints_this_block.push(count_key);
            }

            if let Some(required_excess) = op.required_kernel_excess {
                let satisfied = self.kernel_excesses.contains(&required_excess)
                    || block_kernel_excesses.contains(&required_excess);
                if !satisfied {
                    return None;
                }
            }

            candidate_asset_registry.insert(op.asset_id.clone(), AssetRecord {
                asset_id: op.asset_id.clone(),
                owner_pubkey: op.owner_pubkey,
                metadata: op.metadata.clone(),
                minted_at_block: block.header.height,
                collection_id: op.collection_id.clone(),
            });
        }

        // 4e. Validate asset transfers - same shape as 4c. A transfer's
        // asset_id always refers to an AssetRecord from an earlier block
        // (assets_this_block, shared with the mint loop above, blocks a
        // same-block mint+transfer of the same asset), so its collection
        // (if any) was necessarily already launched too - self.collection_registry
        // is enough, no need for the candidate_collection_registry built
        // for same-block launches above.
        for op in &block.transfer_asset_ops {
            if !assets_this_block.insert(op.asset_id.as_str()) {
                return None;
            }
            let current = match self.asset_registry.get(&op.asset_id) {
                Some(r) => r,
                None => return None,
            };
            let msg = super::assets::TransferAssetOp::signing_message(&op.asset_id, &op.new_owner_pubkey, &op.required_kernel_excess, &op.required_royalty_kernel_excess);
            if !op.signature.verify(&msg, &current.owner_pubkey) {
                return None;
            }
            // The entire consensus-level trustlessness guarantee for
            // marketplace atomic swaps: a conditional transfer literally
            // cannot apply until the payment it's conditioned on already
            // exists on-chain - either from an earlier block, or from this
            // same one (see block_kernel_excesses above).
            if let Some(required_excess) = op.required_kernel_excess {
                let satisfied = self.kernel_excesses.contains(&required_excess)
                    || block_kernel_excesses.contains(&required_excess);
                if !satisfied {
                    return None;
                }
            }
            // Secondary-sale royalty: if the asset's collection charges one,
            // the creator's cut is a second, independent trustless-payment
            // condition - both it and the seller's own payment must be
            // satisfied before the transfer applies. See
            // TransferAssetOp::required_royalty_kernel_excess's doc comment.
            // Only applies to an actual SALE (required_kernel_excess is
            // Some) - an unconditional transfer (a gift, an airdrop, moving
            // an asset between your own wallets) has no sale price to take
            // a cut of, so it must stay free even for a royalty-bearing
            // asset. Gating this on required_kernel_excess rather than on
            // the collection alone is what keeps that possible.
            if op.required_kernel_excess.is_some() {
                if let Some(collection_id) = &current.collection_id {
                    if let Some(collection) = self.collection_registry.get(collection_id) {
                        if collection.royalty_bps > 0 {
                            let Some(required_royalty) = op.required_royalty_kernel_excess else { return None };
                            let satisfied = self.kernel_excesses.contains(&required_royalty)
                                || block_kernel_excesses.contains(&required_royalty);
                            if !satisfied {
                                return None;
                            }
                        }
                    }
                }
            }
            candidate_asset_registry.insert(op.asset_id.clone(), AssetRecord {
                asset_id: op.asset_id.clone(),
                owner_pubkey: op.new_owner_pubkey,
                metadata: current.metadata.clone(),
                minted_at_block: current.minted_at_block,
                collection_id: current.collection_id.clone(),
            });
        }

        if compute_asset_registry_root(&candidate_asset_registry) != block.header.asset_registry_root {
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

        // 7. Add new outputs to the UTXO set. new_outputs_all accumulates
        // every output this block creates (main body + every fee-paying
        // op's change) for AppliedDelta/persist_applied - unlike self.utxos
        // (in-memory, correct for the life of this process), the persisted
        // utxos sled tree is only ever updated via AppliedDelta.new_outputs,
        // so anything missing here is invisible after any restart even
        // though it was never actually spent (the bug behind this session's
        // live faucet investigation - a name/asset registration's real
        // change became permanently unspendable on restart, not because it
        // was ever spent, but because it was never written to disk).
        let mut new_outputs_all: Vec<Output> = block.body.outputs.clone();
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
                new_outputs_all.push(output.clone());
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

        // 7c-prep. Commit each launched collection into the registry - no
        // fee-payment/UTXO involvement, just an insert (see 4c-prep).
        let mut new_collections = Vec::new();
        for op in &block.launch_collection_ops {
            let record = candidate_collection_registry[&op.collection_id].clone();
            self.collection_registry.insert(op.collection_id.clone(), record.clone());
            new_collections.push((op.collection_id.clone(), record));
        }

        // 7d. Apply each asset mint's fee-payment transaction and commit the
        // registry entry itself - same shape as 7b. Collection-tagged mints
        // also commit their incremented per-wallet mint count here.
        let mut new_assets = Vec::new();
        for op in &block.mint_ops {
            for input in &op.fee_payment.inputs {
                self.utxos.remove(&input.commitment);
                spent_commitments.push(input.commitment);
            }
            for output in &op.fee_payment.outputs {
                self.utxos.insert(output.commitment);
                new_outputs_all.push(output.clone());
            }
            let record = candidate_asset_registry[&op.asset_id].clone();
            self.asset_registry.insert(op.asset_id.clone(), record.clone());
            new_assets.push((op.asset_id.clone(), record));
        }
        for count_key in &collection_mints_this_block {
            self.collection_mint_counts.insert(count_key.clone(), candidate_mint_counts[count_key]);
        }

        // 7e. Apply asset transfers - same shape as 7c.
        let mut transferred_assets = Vec::new();
        for op in &block.transfer_asset_ops {
            let old_record = self.asset_registry[&op.asset_id].clone();
            let new_record = candidate_asset_registry[&op.asset_id].clone();
            self.asset_registry.insert(op.asset_id.clone(), new_record);
            transferred_assets.push((op.asset_id.clone(), old_record));
        }
        if !transferred_assets.is_empty() {
            self.asset_transfer_snapshots.insert(block.header.height, transferred_assets.clone());
        }

        // 8. Save the kernels forever. new_kernels_all mirrors
        // new_outputs_all above - AppliedDelta.new_kernels is what actually
        // gets persisted to the disk-backed kernels tree (see
        // storage::persist_applied), so anything missing here is invisible
        // after a restart even though self.kernels (in-memory) has it.
        let mut new_kernels_all: Vec<TxKernel> = block.body.kernels.clone();
        for kernel in &block.body.kernels {
            self.kernels.push(kernel.clone());
            self.kernel_excesses.insert(kernel.excess);
        }
        for op in &block.name_ops {
            for kernel in &op.fee_payment.kernels {
                self.kernels.push(kernel.clone());
                self.kernel_excesses.insert(kernel.excess);
                new_kernels_all.push(kernel.clone());
            }
        }
        for op in &block.mint_ops {
            for kernel in &op.fee_payment.kernels {
                self.kernels.push(kernel.clone());
                self.kernel_excesses.insert(kernel.excess);
                new_kernels_all.push(kernel.clone());
            }
        }

        self.current_height = block.header.height;
        self.last_block_hash = block.header.hash();
        self.blocks.insert(self.last_block_hash, block.clone());

        Some(AppliedDelta {
            block: block.clone(),
            spent_commitments,
            new_outputs: new_outputs_all,
            new_kernels: new_kernels_all,
            new_names,
            transferred_names,
            new_assets,
            transferred_assets,
            new_collections,
            collection_mints_this_block,
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
            self.kernel_excesses.remove(&kernel.excess);
            removed_kernel_excesses.push(kernel.excess);
        }

        // 3b. Revert each name op's fee-payment transaction and registry
        // entry. un_consumed_outputs/removed_kernel_excesses must include
        // these too - they're what persist_rollback actually writes to the
        // disk-backed utxos/kernels trees (see the matching apply-side fix
        // and its comment for why this matters).
        let mut removed_names = Vec::new();
        for op in &tip_block.name_ops {
            for output in &op.fee_payment.outputs {
                self.utxos.remove(&output.commitment);
                un_consumed_outputs.push(output.commitment);
            }
            for input in &op.fee_payment.inputs {
                self.utxos.insert(input.commitment);
                restored_inputs.push(input.commitment);
            }
            for kernel in &op.fee_payment.kernels {
                self.kernels.retain(|k| k.excess != kernel.excess);
                self.kernel_excesses.remove(&kernel.excess);
                removed_kernel_excesses.push(kernel.excess);
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

        // 3d. Revert each asset mint's fee-payment transaction and registry
        // entry - same shape as 3b. A collection-tagged mint also decrements
        // (or removes, if it hits 0) the per-wallet mint count it had
        // incremented at apply time, so a previously-over-limit mint becomes
        // acceptable again post-rollback if this was the mint that had
        // consumed the last slot.
        let mut removed_assets = Vec::new();
        for op in &tip_block.mint_ops {
            for output in &op.fee_payment.outputs {
                self.utxos.remove(&output.commitment);
                un_consumed_outputs.push(output.commitment);
            }
            for input in &op.fee_payment.inputs {
                self.utxos.insert(input.commitment);
                restored_inputs.push(input.commitment);
            }
            for kernel in &op.fee_payment.kernels {
                self.kernels.retain(|k| k.excess != kernel.excess);
                self.kernel_excesses.remove(&kernel.excess);
                removed_kernel_excesses.push(kernel.excess);
            }
            self.asset_registry.remove(&op.asset_id);
            removed_assets.push(op.asset_id.clone());

            if let (Some(collection_id), Some(phase_index)) = (&op.collection_id, op.phase_index) {
                let owner_bytes = *op.owner_pubkey.as_point().compress().as_bytes();
                let count_key = (collection_id.clone(), phase_index, owner_bytes);
                if let Some(count) = self.collection_mint_counts.get_mut(&count_key) {
                    if *count <= 1 {
                        self.collection_mint_counts.remove(&count_key);
                    } else {
                        *count -= 1;
                    }
                }
            }
        }

        // 3d-prep. Revert each launched collection - pure removal, no
        // fee-payment/UTXO involvement (see 4c-prep/7c-prep).
        let mut removed_collections = Vec::new();
        for op in &tip_block.launch_collection_ops {
            self.collection_registry.remove(&op.collection_id);
            removed_collections.push(op.collection_id.clone());
        }

        // 3e. Revert each asset transfer back to its pre-transfer record.
        if let Some(pre_transfer) = self.asset_transfer_snapshots.remove(&self.current_height) {
            for (asset_id, old_record) in pre_transfer {
                self.asset_registry.insert(asset_id, old_record);
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
            removed_assets,
            removed_collections,
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

/// Sums every name-registration/asset-mint fee ever paid (see
/// aggregate_validate's doc for why this exact correction term is needed).
/// Safe to run against a partially-pruned `blocks` map: compact() never
/// strips fee_payment.kernels, only fee_payment.inputs/outputs.
pub fn total_registry_fees_burned(blocks: &HashMap<[u8; 32], Block>) -> u64 {
    blocks.values()
        .flat_map(|b| {
            b.name_ops.iter().flat_map(|op| op.fee_payment.kernels.iter())
                .chain(b.mint_ops.iter().flat_map(|op| op.fee_payment.kernels.iter()))
        })
        .map(|k| k.fee)
        .sum()
}

/// Grin-style aggregate validation for chain history that's been
/// horizon-pruned (see core::compaction): proves no value was created or
/// destroyed outside the reward schedule, and that every contributing
/// kernel excess is backed by a real signature - using only data that
/// survives cut-through pruning (the flat kernel list and the current
/// UTXO set), not any specific historical input/output.
///
/// Derivation: every block's body satisfies (Transaction::
/// validate_with_reward) `Σ C_in − Σ C_out − fee·H + reward·H = Σ excess`,
/// with fee forced to 0 whenever reward > 0 (i.e. whenever validating a
/// real mined block, not a standalone reward=0 transaction). Telescoping
/// this over every block from genesis to tip, every input ever spent is
/// some earlier output, so `Σ(all C_in) − Σ(all C_out)` collapses to
/// `−Σ(current UTXO set)`, leaving:
///
///   Σ(current UTXOs) + Σ(all kernel excess, genesis..tip) == total_reward_issued · H
///
/// One correction the reward schedule alone doesn't capture: name/asset
/// registration fees are validated standalone with reward=0, so they're
/// genuinely subtracted rather than reinjected into a later coinbase the
/// way ordinary payment fees are (core::proposer's total_fees only sums
/// tx.kernels, never name_ops/mint_ops fee-payment kernels) - they're
/// simply burned. Hence `registry_fees_burned` (see
/// total_registry_fees_burned) is subtracted from the issued total.
///
/// This is strictly weaker than full per-block replay in one respect -
/// proposer/validator legitimacy for the pruned range isn't re-derivable
/// this way, since active_validators' history is tied to exactly the
/// input/output history that gets pruned. See the design writeup for the
/// full list of what this does and doesn't close.
pub fn aggregate_validate(
    utxos: &HashSet<Commitment>,
    kernels: &[TxKernel],
    tip_height: u64,
    registry_fees_burned: u64,
) -> bool {
    // Every excess must be backed by a real signature - without this, an
    // attacker could pick an excess point that merely makes the sum below
    // balance, with no known discrete log/blinding factor behind it at all.
    for k in kernels {
        if !k.signature.verify(&k.fee.to_le_bytes(), &k.excess) {
            return false;
        }
    }

    let mut sum = curve25519_dalek_ng::ristretto::RistrettoPoint::default();
    for c in utxos {
        sum += c.as_point();
    }
    for k in kernels {
        sum += k.excess.as_point();
    }

    let total_reward_issued: u64 = super::genesis::GENESIS_TOTAL_MINTED
        + (1..=tip_height).map(super::block::block_reward_at).sum::<u64>();
    let net_issued = total_reward_issued.saturating_sub(registry_fees_burned);

    sum == Commitment::new(net_issued, Scalar::zero()).as_point()
}
