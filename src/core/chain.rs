use std::collections::HashSet;
use crate::crypto::pedersen::Commitment;
use super::transaction::{TxKernel, Output};
use super::block::Block;

/// Maintains the global state of the Mimblewimble blockchain.
#[derive(Debug, Default)]
pub struct ChainState {
    /// The Unspent Transaction Output (UTXO) set.
    pub utxos: HashSet<Commitment>,
    /// We also store unspent outputs (commitments + range proofs) to serve to syncing nodes
    pub unspent_outputs: Vec<Output>,
    /// All transaction kernels ever recorded on the chain
    pub kernels: Vec<TxKernel>,
    pub current_height: u64,
    pub last_block_hash: [u8; 32],
}

impl ChainState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempts to apply a new block to the chain state.
    /// Returns true if successful, false if the block is invalid.
    pub fn apply_block(&mut self, block: &Block) -> bool {
        // 1. Verify the block's internal cryptography
        if !block.validate() {
            return false;
        }

        // 2. Ensure all inputs exist in our UTXO set (no double spends, no fake inputs)
        for input in &block.body.inputs {
            if !self.utxos.contains(&input.commitment) {
                return false;
            }
        }

        // 3. Remove spent inputs from the UTXO set
        for input in &block.body.inputs {
            self.utxos.remove(&input.commitment);
        }

        // 4. Add new outputs to the UTXO set
        for output in &block.body.outputs {
            self.utxos.insert(output.commitment);
            self.unspent_outputs.push(output.clone());
        }

        // 5. Save the kernels forever
        for kernel in &block.body.kernels {
            self.kernels.push(kernel.clone());
        }

        self.current_height = block.header.height;
        self.last_block_hash = block.header.hash();
        true
    }
}
