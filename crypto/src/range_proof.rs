use bulletproofs::{BulletproofGens, PedersenGens, RangeProof as BpRangeProof};
use curve25519_dalek_ng::scalar::Scalar;
use merlin::Transcript;

use super::pedersen::Commitment;

const RANGE_BIT_LENGTH: usize = 64; // Proof that value is within 0..2^64-1

use serde::{Serialize, Serializer, Deserialize, Deserializer};

#[derive(Clone, Debug)]
pub struct RangeProof(pub BpRangeProof);

impl Serialize for RangeProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0.to_bytes())
    }
}

impl<'de> Deserialize<'de> for RangeProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        let bp_proof = BpRangeProof::from_bytes(&bytes).map_err(|_| serde::de::Error::custom("Invalid range proof bytes"))?;
        Ok(RangeProof(bp_proof))
    }
}

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
            RANGE_BIT_LENGTH,
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
            RANGE_BIT_LENGTH,
        ).is_ok()
    }
}
