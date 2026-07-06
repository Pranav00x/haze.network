use std::collections::{HashMap, HashSet};
use sled::{Db, Tree, Batch};

use crate::crypto::pedersen::Commitment;
use crate::core::transaction::TxKernel;
use super::chain::{ChainState, Validator, AppliedDelta, RollbackDelta};
use super::compaction::BlockPruneMeta;
use super::block::Block;

const DEFAULT_DATA_DIR: &str = "haze_data";

const META_HEIGHT_KEY: &[u8] = b"height";
const META_TIP_KEY: &[u8] = b"tip";
const META_VALIDATORS_KEY: &[u8] = b"active_validators";

pub struct Storage {
    blocks: Tree,
    utxos: Tree,
    kernels: Tree,
    validator_snapshots: Tree,
    /// Per-block cut-through bookkeeping (see core::compaction) - keyed by
    /// block hash, same as `blocks`. Only ever written by persist_compaction.
    prune_meta: Tree,
    meta: Tree,
}

impl Storage {
    /// Opens the db under `$HAZE_DATA_DIR/db` if set (e.g. a mounted persistent
    /// disk on a hosting platform), otherwise `haze_data/db` relative to the
    /// process's cwd - same default as before this env var existed.
    pub fn open() -> Self {
        let data_dir = std::env::var("HAZE_DATA_DIR").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string());
        Self::open_at(&format!("{}/db", data_dir.trim_end_matches('/')))
    }

    /// Opens (or creates) a sled database at a specific path. Exposed mainly so
    /// tests can each use their own isolated path instead of contending for the
    /// single exclusive lock sled holds on `haze_data/db`.
    pub fn open_at(path: &str) -> Self {
        let db: Db = sled::open(path).expect("Failed to open sled database");
        Self {
            blocks: db.open_tree("blocks").expect("Failed to open blocks tree"),
            utxos: db.open_tree("utxos").expect("Failed to open utxos tree"),
            kernels: db.open_tree("kernels").expect("Failed to open kernels tree"),
            validator_snapshots: db.open_tree("validator_snapshots").expect("Failed to open validator_snapshots tree"),
            prune_meta: db.open_tree("prune_meta").expect("Failed to open prune_meta tree"),
            meta: db.open_tree("meta").expect("Failed to open meta tree"),
        }
    }

    /// Reconstructs the full ChainState from disk. An empty database yields ChainState::new().
    pub fn load_state(&self) -> ChainState {
        let mut state = ChainState::new();

        for entry in self.blocks.iter() {
            if let Ok((hash_bytes, block_bytes)) = entry {
                if let Ok(block) = bincode::deserialize::<Block>(&block_bytes) {
                    let mut hash = [0u8; 32];
                    if hash_bytes.len() == 32 {
                        hash.copy_from_slice(&hash_bytes);
                        state.blocks.insert(hash, block);
                    }
                }
            }
        }

        let mut utxos: HashSet<Commitment> = HashSet::new();
        for entry in self.utxos.iter() {
            if let Ok((commitment_bytes, _)) = entry {
                if let Some(commitment) = Commitment::from_compressed_bytes(&commitment_bytes) {
                    utxos.insert(commitment);
                }
            }
        }
        state.utxos = utxos;

        let mut kernels: Vec<TxKernel> = Vec::new();
        for entry in self.kernels.iter() {
            if let Ok((_, kernel_bytes)) = entry {
                if let Ok(kernel) = bincode::deserialize::<TxKernel>(&kernel_bytes) {
                    kernels.push(kernel);
                }
            }
        }
        state.kernel_excesses = kernels.iter().map(|k| k.excess).collect();
        state.kernels = kernels;

        let mut validator_snapshots: HashMap<u64, Vec<Validator>> = HashMap::new();
        for entry in self.validator_snapshots.iter() {
            if let Ok((height_bytes, vals_bytes)) = entry {
                if height_bytes.len() == 8 {
                    let mut h = [0u8; 8];
                    h.copy_from_slice(&height_bytes);
                    let height = u64::from_be_bytes(h);
                    if let Ok(vals) = bincode::deserialize::<Vec<Validator>>(&vals_bytes) {
                        validator_snapshots.insert(height, vals);
                    }
                }
            }
        }
        state.validator_snapshots = validator_snapshots;

        let mut prune_meta: HashMap<[u8; 32], BlockPruneMeta> = HashMap::new();
        for entry in self.prune_meta.iter() {
            if let Ok((hash_bytes, meta_bytes)) = entry {
                if hash_bytes.len() == 32 {
                    let mut hash = [0u8; 32];
                    hash.copy_from_slice(&hash_bytes);
                    if let Ok(meta) = bincode::deserialize::<BlockPruneMeta>(&meta_bytes) {
                        prune_meta.insert(hash, meta);
                    }
                }
            }
        }
        state.prune_meta = prune_meta;

        if let Ok(Some(height_bytes)) = self.meta.get(META_HEIGHT_KEY) {
            if height_bytes.len() == 8 {
                let mut h = [0u8; 8];
                h.copy_from_slice(&height_bytes);
                state.current_height = u64::from_be_bytes(h);
            }
        }

        if let Ok(Some(tip_bytes)) = self.meta.get(META_TIP_KEY) {
            if tip_bytes.len() == 32 {
                let mut tip = [0u8; 32];
                tip.copy_from_slice(&tip_bytes);
                state.last_block_hash = tip;
            }
        }

        if let Ok(Some(validators_bytes)) = self.meta.get(META_VALIDATORS_KEY) {
            if let Ok(validators) = bincode::deserialize::<Vec<Validator>>(&validators_bytes) {
                state.active_validators = validators;
            }
        }

        // Rebuild the name registry by replaying name_ops/transfer_ops from
        // blocks on the ACTIVE chain only - no separate sled tree needed,
        // since full blocks are already persisted above. Two passes:
        // 1) walk back from the tip via prev_hash (must not include
        //    orphaned/rolled-back blocks, which stay in `state.blocks`
        //    forever for potential reorgs but shouldn't count here),
        // 2) reverse to chronological (genesis-first) order before
        //    replaying, since a transfer must apply *after* the register
        //    it targets, not in arbitrary hash-map order.
        let mut active_chain_blocks = Vec::new();
        let mut cursor = state.last_block_hash;
        while cursor != [0u8; 32] {
            let Some(block) = state.blocks.get(&cursor) else { break };
            let height = block.header.height;
            let prev = block.header.prev_hash;
            active_chain_blocks.push(block.clone());
            cursor = prev;
            if height == 0 {
                break;
            }
        }
        active_chain_blocks.reverse();

        for block in &active_chain_blocks {
            for op in &block.name_ops {
                state.name_registry.insert(op.name.clone(), super::registry::NameRecord {
                    name: op.name.clone(),
                    owner_pubkey: op.owner_pubkey,
                    resolves_to: op.resolves_to,
                    registered_at_block: block.header.height,
                });
            }
            for op in &block.transfer_ops {
                if let Some(existing) = state.name_registry.get(&op.name) {
                    let updated = super::registry::NameRecord {
                        name: op.name.clone(),
                        owner_pubkey: op.new_owner_pubkey,
                        resolves_to: op.new_resolves_to,
                        registered_at_block: existing.registered_at_block,
                    };
                    state.name_registry.insert(op.name.clone(), updated);
                }
            }
            // Asset registry (see core::assets) rebuilt the same way, from
            // the same already-persisted blocks - no separate sled tree
            // needed, same reasoning as name_registry above.
            for op in &block.mint_ops {
                state.asset_registry.insert(op.asset_id.clone(), super::assets::AssetRecord {
                    asset_id: op.asset_id.clone(),
                    owner_pubkey: op.owner_pubkey,
                    metadata: op.metadata.clone(),
                    minted_at_block: block.header.height,
                });
            }
            for op in &block.transfer_asset_ops {
                if let Some(existing) = state.asset_registry.get(&op.asset_id) {
                    let updated = super::assets::AssetRecord {
                        asset_id: op.asset_id.clone(),
                        owner_pubkey: op.new_owner_pubkey,
                        metadata: existing.metadata.clone(),
                        minted_at_block: existing.minted_at_block,
                    };
                    state.asset_registry.insert(op.asset_id.clone(), updated);
                }
            }
        }

        state
    }

    /// Persists a single successfully-applied block delta: only the keys that changed.
    pub fn persist_applied(&self, delta: &AppliedDelta) -> sled::Result<()> {
        let block_hash = delta.tip_hash;
        self.blocks.insert(&block_hash[..], bincode::serialize(&delta.block).unwrap())?;

        let mut utxo_batch = Batch::default();
        for commitment in &delta.spent_commitments {
            utxo_batch.remove(commitment_key(commitment).as_slice());
        }
        for output in &delta.new_outputs {
            utxo_batch.insert(commitment_key(&output.commitment), bincode::serialize(output).unwrap());
        }
        self.utxos.apply_batch(utxo_batch)?;

        let mut kernel_batch = Batch::default();
        for kernel in &delta.new_kernels {
            kernel_batch.insert(commitment_key(&kernel.excess), bincode::serialize(kernel).unwrap());
        }
        self.kernels.apply_batch(kernel_batch)?;

        let (snap_height, snap_validators) = &delta.validator_snapshot;
        self.validator_snapshots.insert(&snap_height.to_be_bytes(), bincode::serialize(snap_validators).unwrap())?;

        self.meta.insert(META_HEIGHT_KEY, &delta.height.to_be_bytes())?;
        self.meta.insert(META_TIP_KEY, &delta.tip_hash[..])?;
        self.meta.insert(META_VALIDATORS_KEY, bincode::serialize(&delta.active_validators_after).unwrap())?;

        Ok(())
    }

    /// Persists a single rollback delta: only the keys that changed.
    pub fn persist_rollback(&self, delta: &RollbackDelta) -> sled::Result<()> {
        let mut utxo_batch = Batch::default();
        for commitment in &delta.un_consumed_outputs {
            utxo_batch.remove(commitment_key(commitment).as_slice());
        }
        // Restored inputs go back into the UTXO set; we don't have the original Output
        // (with its range proof) handy here, but it's still present in the blocks tree
        // under the block that originally created it, so callers needing the full
        // Output can look it up there. For the utxos tree we only need the membership
        // marker used by get/contains-style checks in load_state, so store an empty value.
        for commitment in &delta.restored_inputs {
            utxo_batch.insert(commitment_key(commitment), Vec::new());
        }
        self.utxos.apply_batch(utxo_batch)?;

        let mut kernel_batch = Batch::default();
        for excess in &delta.removed_kernel_excesses {
            kernel_batch.remove(commitment_key(excess).as_slice());
        }
        self.kernels.apply_batch(kernel_batch)?;

        self.meta.insert(META_HEIGHT_KEY, &delta.new_height.to_be_bytes())?;
        self.meta.insert(META_TIP_KEY, &delta.new_tip[..])?;
        self.meta.insert(META_VALIDATORS_KEY, bincode::serialize(&delta.active_validators_after).unwrap())?;

        Ok(())
    }

    /// Persists the current active validator set directly. Used for the
    /// register_validator path, which mutates active_validators outside of block
    /// application (small, rewritten wholesale - cheap compared to the collections above).
    pub fn persist_active_validators(&self, validators: &[Validator]) -> sled::Result<()> {
        self.meta.insert(META_VALIDATORS_KEY, bincode::serialize(validators).unwrap())?;
        Ok(())
    }

    /// Persists a completed horizon-based cut-through pass (see
    /// core::compaction::compact): re-writes just the blocks that were
    /// actually trimmed (now missing some inputs/outputs, same hash/key as
    /// before) plus their prune_meta entries, and drops validator snapshots
    /// older than the horizon from disk too - otherwise a restart would
    /// reload them from `validator_snapshots` and silently undo that part
    /// of the in-memory pruning on every load_state.
    pub fn persist_compaction(&self, chain: &ChainState, touched_blocks: &[[u8; 32]]) -> sled::Result<()> {
        for hash in touched_blocks {
            if let Some(block) = chain.blocks.get(hash) {
                self.blocks.insert(&hash[..], bincode::serialize(block).unwrap())?;
            }
            if let Some(meta) = chain.prune_meta.get(hash) {
                self.prune_meta.insert(&hash[..], bincode::serialize(meta).unwrap())?;
            }
        }

        // validator_snapshots is keyed by big-endian height, so a lexicographic
        // range below the horizon is exactly the numeric range below it.
        let horizon_height = chain.current_height.saturating_sub(super::compaction::CUT_THROUGH_HORIZON);
        let mut snapshot_batch = Batch::default();
        for entry in self.validator_snapshots.range(..horizon_height.to_be_bytes().to_vec()) {
            if let Ok((key, _)) = entry {
                snapshot_batch.remove(key);
            }
        }
        self.validator_snapshots.apply_batch(snapshot_batch)?;

        Ok(())
    }
}

fn commitment_key(commitment: &Commitment) -> Vec<u8> {
    commitment.as_point().compress().as_bytes().to_vec()
}
