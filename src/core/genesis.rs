use crate::core::block::{Block, BlockHeader};
use crate::core::transaction::{Transaction, Output, TxKernel};
use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use curve25519_dalek_ng::scalar::Scalar;

/// Computes and returns the hardcoded Genesis block for Haze.
/// Allocates 1,000,000 haze to a single genesis UTXO with a known blinding factor (Scalar(42)).
pub fn genesis_block() -> Block {
    let genesis_val = 1_000_000u64;
    let genesis_blinding = Scalar::from(42u64);
    
    let commitment = Commitment::new(genesis_val, genesis_blinding);
    let proof = RangeProof::prove(genesis_val, &genesis_blinding);
    
    let output = Output {
        commitment,
        proof,
    };
    
    // Create a genesis kernel with 0 fee.
    let excess_commitment = Commitment::new(0, genesis_blinding);
    let signature = Signature::sign(&0u64.to_le_bytes(), &genesis_blinding);
    let kernel = TxKernel {
        excess: excess_commitment,
        fee: 0,
        signature,
    };
    
    let body = Transaction {
        inputs: vec![],
        outputs: vec![output],
        kernels: vec![kernel],
    };
    
    Block {
        header: BlockHeader {
            height: 0,
            prev_hash: [0u8; 32],
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(0, Scalar::zero()),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
        },
        body,
    }
}
