use std::collections::{HashMap, HashSet};
use sled::{Db, Tree, Batch};

use crate::crypto::pedersen::Commitment;
use crate::core::transaction::TxKernel;
use super::chain::{ChainState, Validator, AppliedDelta, RollbackDelta};
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

        // Rebuild the name registry by replaying name_ops from blocks on the
        // ACTIVE chain only (walking back from the tip via prev_hash) - no
        // separate sled tree needed, since full blocks (including name_ops)
        // are already persisted above. Must not include orphaned/rolled-back
        // blocks, which stay in `state.blocks` forever for potential reorgs
        // but shouldn't count toward current registry state.
        let mut cursor = state.last_block_hash;
        while cursor != [0u8; 32] {
            let Some(block) = state.blocks.get(&cursor) else { break };
            for op in &block.name_ops {
                state.name_registry.insert(op.name.clone(), super::registry::NameRecord {
                    name: op.name.clone(),
                    owner_pubkey: op.owner_pubkey,
                    resolves_to: op.resolves_to,
                    registered_at_block: block.header.height,
                });
            }
            let height = block.header.height;
            cursor = block.header.prev_hash;
            if height == 0 {
                break;
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
}

fn commitment_key(commitment: &Commitment) -> Vec<u8> {
    commitment.as_point().compress().as_bytes().to_vec()
}
