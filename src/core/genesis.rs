use crate::core::block::{Block, BlockHeader};
use crate::core::transaction::{Transaction, Output, TxKernel};
use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use curve25519_dalek_ng::scalar::Scalar;

/// Total value minted at genesis (validator stake output + faucet reserve) -
/// chain.rs's genesis validation must use this as the expected block reward.
pub const GENESIS_TOTAL_MINTED: u64 = 1_000_000 + FAUCET_RESERVE_VALUE;

/// A separate well-known devnet secret (distinct from the Scalar(42) validator
/// stake output) reserved to fund the node's own faucet identity (see
/// src/api/faucet.rs) - unlike the single-use claim-genesis output, this one
/// is only ever spent by the node itself, which hands out change from it.
pub const FAUCET_RESERVE_BLINDING: u64 = 43;
pub const FAUCET_RESERVE_VALUE: u64 = 50_000_000;

fn mint_output(value: u64, blinding: Scalar) -> (Output, TxKernel) {
    let commitment = Commitment::new(value, blinding);
    let proof = RangeProof::prove(value, &blinding);
    let output = Output { commitment, proof };

    // No corresponding input, so the excess blinding factor is just the
    // negation of the output's own blinding factor.
    let excess_blinding = Scalar::zero() - blinding;
    let excess_commitment = Commitment::new(0, excess_blinding);
    let signature = Signature::sign(&0u64.to_le_bytes(), &excess_blinding);
    let kernel = TxKernel { excess: excess_commitment, fee: 0, signature };

    (output, kernel)
}

/// Computes and returns the hardcoded Genesis block for Haze. Mints two known
/// devnet outputs: the validator stake / claim-genesis output (1,000,000,
/// blinding=42) and a separate faucet reserve (blinding=43) that funds the
/// node's own repeatable devnet faucet.
pub fn genesis_block() -> Block {
    let genesis_val = 1_000_000u64;
    let genesis_blinding = Scalar::from(42u64);

    let (validator_output, validator_kernel) = mint_output(genesis_val, genesis_blinding);
    let (faucet_output, faucet_kernel) = mint_output(FAUCET_RESERVE_VALUE, Scalar::from(FAUCET_RESERVE_BLINDING));

    let body = Transaction {
        inputs: vec![],
        outputs: vec![validator_output, faucet_output],
        kernels: vec![validator_kernel, faucet_kernel],
    };

    Block {
        header: BlockHeader {
            height: 0,
            prev_hash: [0u8; 32],
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(genesis_val, genesis_blinding),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: super::registry::compute_registry_root(&std::collections::HashMap::new()),
        },
        body,
        name_ops: vec![],
        transfer_ops: vec![],
    }
}
