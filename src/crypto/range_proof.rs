use bulletproofs::{BulletproofGens, PedersenGens, RangeProof as BpRangeProof};
use curve25519_dalek::scalar::Scalar;
use merlin::Transcript;

use super::pedersen::Commitment;

const RANGE_BIT_LENGTH: usize = 64; // Proof that value is within 0..2^64-1

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeProof(pub BpRangeProof);

impl RangeProof {
    /// Create a range proof for a value and its blinding factor
    pub fn prove(value: u64, blinding: &Scalar) -> Self {
        let pc_gens = PedersenGens::default();
        let bp_gens = BulletproofGens::new(RANGE_BIT_LENGTH, 1);
        let mut transcript = Transcript::new(b"Haze Range Proof");
        
        let (proof, _commitment) = BpRangeProof::prove_single(
            &bp_gens,
            &pc_gens,
            &mut transcript,
            value,
            blinding,
        ).expect("A range proof should always generate successfully");
        
        RangeProof(proof)
    }

    /// Verify a range proof against a commitment
    pub fn verify(&self, commitment: &Commitment) -> bool {
        let pc_gens = PedersenGens::default();
        let bp_gens = BulletproofGens::new(RANGE_BIT_LENGTH, 1);
        let mut transcript = Transcript::new(b"Haze Range Proof");
        
        let compressed = commitment.as_point().compress();
        
        self.0.verify_single(
            &bp_gens,
            &pc_gens,
            &mut transcript,
            &compressed,
            1,
        ).is_ok()
    }
}
