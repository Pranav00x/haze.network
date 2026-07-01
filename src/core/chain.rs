use std::collections::{HashSet, HashMap};
use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use super::transaction::{TxKernel, Output};
use super::block::Block;
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
    /// We also store unspent outputs (commitments + range proofs) to serve to syncing nodes
    pub unspent_outputs: Vec<Output>,
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

    /// Deterministically selects the block proposer for a given height and previous hash.
    pub fn select_proposer(&self, height: u64, prev_hash: [u8; 32]) -> Commitment {
        if self.active_validators.is_empty() {
            // Default to genesis validator commitment
            let genesis_blinding = Scalar::from(42u64);
            return Commitment::new(1_000_000, genesis_blinding);
        }

        let total_stake: u64 = self.active_validators.iter().map(|v| v.value).sum();
        if total_stake == 0 {
            return self.active_validators[0].commitment;
        }

        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&height.to_le_bytes());
        hasher.update(&prev_hash);
        let hash = hasher.finalize();

        let mut seed_bytes = [0u8; 8];
        seed_bytes.copy_from_slice(&hash[0..8]);
        let seed = u64::from_le_bytes(seed_bytes);

        let mut target = seed % total_stake;
        for validator in &self.active_validators {
            if target < validator.value {
                return validator.commitment;
            }
            target -= validator.value;
        }

        self.active_validators[0].commitment
    }

    /// Attempts to apply a new block to the chain state.
    /// Triggers a reorganization if the block is on a heavier/taller fork.
    pub fn apply_block(&mut self, block: &Block) -> bool {
        let block_hash = block.header.hash();
        
        // Store block in block database first
        self.blocks.insert(block_hash, block.clone());

        // Case 1: Simple linear application on top of current chaintip (or genesis block on fresh tip)
        let is_linear = (block.header.height == self.current_height + 1 && block.header.prev_hash == self.last_block_hash)
            || (block.header.height == 0 && self.current_height == 0 && self.last_block_hash == [0u8; 32]);
            
        if is_linear {
            return self.apply_linear_block(block);
        }

        // Case 2: Block is on a fork
        // Trigger reorg only if the fork block is taller/heavier than our current tip
        if block.header.height > self.current_height {
            if let Some((rollback_hashes, fork_blocks)) = self.find_reorg_path(block) {
                println!("ChainState: Reorganization triggered! Rolling back {} blocks, applying {} fork blocks", rollback_hashes.len(), fork_blocks.len());
                let mut sandbox = self.clone();
                
                // Roll back current tip to common ancestor
                for _hash in &rollback_hashes {
                    if !sandbox.rollback_block() {
                        return false;
                    }
                }
                
                // Apply the new fork blocks
                for fb in &fork_blocks {
                    if !sandbox.apply_linear_block(fb) {
                        return false;
                    }
                }
                
                // Reorg successful, commit changes
                *self = sandbox;
                return true;
            }
        }

        // Otherwise, we just store it in self.blocks (done above) and return false
        false
    }

    /// Directly applies a block on top of the current tip.
    pub fn apply_linear_block(&mut self, block: &Block) -> bool {
        // 1. Verify height and prev_hash connectivity
        if block.header.height != self.current_height + 1 {
            // Special case for Genesis Block (height 0) on fresh state
            if !(block.header.height == 0 && self.current_height == 0 && self.last_block_hash == [0u8; 32]) {
                return false;
            }
        } else if block.header.prev_hash != self.last_block_hash {
            return false;
        }

        // 2. Verify Proof of Stake block proposer signature
        let expected_proposer = self.select_proposer(block.header.height, block.header.prev_hash);
        if block.header.validator_commitment != expected_proposer {
            return false;
        }

        let stake_value = if expected_proposer == Commitment::new(1_000_000, Scalar::from(42u64)) {
            1_000_000
        } else {
            self.active_validators.iter()
                .find(|v| v.commitment == expected_proposer)
                .map(|v| v.value)
                .unwrap_or(0)
        };

        let gens = PedersenGens::default();
        let p_sig_point = expected_proposer.as_point() - Scalar::from(stake_value) * gens.B;
        let p_sig_commitment = Commitment(p_sig_point);

        let mut header_copy = block.header.clone();
        header_copy.validator_signature = Signature { s: Scalar::zero(), e: Scalar::zero() };
        let msg = header_copy.hash();

        if block.header.height > 0 && !block.header.validator_signature.verify(&msg, &p_sig_commitment) {
            return false;
        }

        // 3. Verify the block's internal cryptography
        let block_reward = if block.header.height == 0 {
            1_000_000
        } else {
            super::block::BLOCK_REWARD
        };

        if !block.body.validate_with_reward(block_reward) {
            return false;
        }

        // 4. Ensure all inputs exist in our UTXO set (no double spends, no fake inputs)
        // Skip input checks for genesis block (since it has no inputs)
        if block.header.height > 0 {
            for input in &block.body.inputs {
                if !self.utxos.contains(&input.commitment) {
                    return false;
                }
            }
        }

        // 5. Save the snapshot of active validators BEFORE applying state transitions
        self.validator_snapshots.insert(block.header.height, self.active_validators.clone());

        // 6. Remove spent inputs from the UTXO set and validators
        if block.header.height > 0 {
            for input in &block.body.inputs {
                self.utxos.remove(&input.commitment);
                self.active_validators.retain(|val| val.commitment != input.commitment);
            }
        }

        // 7. Add new outputs to the UTXO set
        for output in &block.body.outputs {
            self.utxos.insert(output.commitment);
            self.unspent_outputs.push(output.clone());
        }

        // 8. Save the kernels forever
        for kernel in &block.body.kernels {
            self.kernels.push(kernel.clone());
        }

        self.current_height = block.header.height;
        self.last_block_hash = block.header.hash();
        self.blocks.insert(self.last_block_hash, block.clone());
        true
    }

    /// Rolls back the chaintip block by one height.
    /// Returns true if successful, false if there are no blocks to roll back.
    pub fn rollback_block(&mut self) -> bool {
        if self.current_height == 0 {
            return false;
        }

        let tip_hash = self.last_block_hash;
        let tip_block = match self.blocks.get(&tip_hash) {
            Some(b) => b.clone(),
            None => return false,
        };

        // 1. Remove output commitments from UTXO set
        for output in &tip_block.body.outputs {
            self.utxos.remove(&output.commitment);
            self.unspent_outputs.retain(|o| o.commitment != output.commitment);
        }

        // 2. Restore spent input commitments back to UTXO set
        for input in &tip_block.body.inputs {
            self.utxos.insert(input.commitment);
        }

        // 3. Remove block kernels
        for kernel in &tip_block.body.kernels {
            self.kernels.retain(|k| k.excess != kernel.excess);
        }

        // 4. Restore active validator registry snapshot from height H-1
        let prev_height = self.current_height - 1;
        self.active_validators = self.validator_snapshots.get(&prev_height).cloned().unwrap_or_default();

        // 5. Clean up snapshot at height H
        self.validator_snapshots.remove(&self.current_height);

        // 6. Update tip metadata
        self.current_height = prev_height;
        self.last_block_hash = tip_block.header.prev_hash;

        true
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

        let has_more = collected.len() > limit;
        if has_more {
            collected.drain(0..collected.len() - limit);
        }
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
