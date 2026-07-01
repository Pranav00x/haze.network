use std::collections::HashSet;
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

/// Maintains the global state of the Mimblewimble blockchain.
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
}

impl ChainState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a staker as an active validator by revealing their stake value and blinding factor.
    pub fn register_validator(&mut self, commitment: Commitment, value: u64, blinding: Scalar) -> bool {
        if Commitment::new(value, blinding) != commitment {
            return false;
        }
        if !self.utxos.contains(&commitment) {
            return false;
        }
        if let Some(pos) = self.active_validators.iter().position(|v| v.commitment == commitment) {
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
    /// Returns true if successful, false if the block is invalid.
    pub fn apply_block(&mut self, block: &Block) -> bool {
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
        if !block.validate() {
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

        // 5. Remove spent inputs from the UTXO set and validators
        if block.header.height > 0 {
            for input in &block.body.inputs {
                self.utxos.remove(&input.commitment);
                self.active_validators.retain(|val| val.commitment != input.commitment);
            }
        }

        // 6. Add new outputs to the UTXO set
        for output in &block.body.outputs {
            self.utxos.insert(output.commitment);
            self.unspent_outputs.push(output.clone());
        }

        // 7. Save the kernels forever
        for kernel in &block.body.kernels {
            self.kernels.push(kernel.clone());
        }

        self.current_height = block.header.height;
        self.last_block_hash = block.header.hash();
        true
    }
}
