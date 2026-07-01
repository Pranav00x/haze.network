use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use super::transaction::Transaction;
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
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    // The entire block is just one giant cut-through transaction
    pub body: Transaction,
}

impl Block {
    /// Validates the block by checking its internal transaction balances and signatures
    pub fn validate(&self) -> bool {
        self.body.validate()
        // In a full implementation, we'd also verify the total_kernel_offset
        // and check proof-of-work, etc.
    }
}
