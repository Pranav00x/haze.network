use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use super::transaction::Transaction;
use super::registry::RegisterNameOp;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub prev_hash: [u8; 32],
    // Mimblewimble offset for the block
    pub total_kernel_offset: curve25519_dalek_ng::scalar::Scalar,
    pub nonce: u64,
    pub timestamp: u64,
    pub validator_commitment: Commitment,
    pub validator_signature: Signature,
    /// Commitment to the full naming-registry state after this block is
    /// applied (see core::registry::compute_registry_root). Separate from
    /// the UTXO set on purpose - name ownership is intentionally public.
    pub name_registry_root: [u8; 32],
}

impl BlockHeader {
    pub fn hash(&self) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.prev_hash);
        hasher.update(self.total_kernel_offset.as_bytes());
        hasher.update(&self.nonce.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(self.validator_commitment.as_point().compress().as_bytes());
        hasher.update(self.validator_signature.s.as_bytes());
        hasher.update(self.validator_signature.e.as_bytes());
        hasher.update(&self.name_registry_root);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}

pub const BLOCK_REWARD: u64 = 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    // The entire block is just one giant cut-through transaction
    pub body: Transaction,
    /// Name registrations included in this block - separate from `body`
    /// since they're not a value-transfer operation, just carry their own
    /// small fee-paying transaction each (see RegisterNameOp).
    #[serde(default)]
    pub name_ops: Vec<RegisterNameOp>,
}

impl Block {
    /// Validates the block by checking its internal transaction balances and signatures.
    /// Name ops are validated separately (ChainState::apply_linear_block) since checking
    /// them fully requires chain state (name uniqueness, real UTXOs) this method doesn't have.
    pub fn validate(&self) -> bool {
        self.body.validate_with_reward(BLOCK_REWARD)
    }
}
