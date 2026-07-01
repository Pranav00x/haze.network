use super::transaction::Transaction;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub prev_hash: [u8; 32],
    // Mimblewimble offset for the block
    pub total_kernel_offset: curve25519_dalek::scalar::Scalar,
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
