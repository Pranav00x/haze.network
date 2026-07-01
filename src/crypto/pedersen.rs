use bulletproofs::PedersenGens;
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use std::ops::{Add, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Commitment(pub RistrettoPoint);

impl Commitment {
    /// Creates a Pedersen commitment to a value `v` with a blinding factor `r`: C = v * H + r * G.
    /// We use the default `PedersenGens` from `bulletproofs` which ensures that `H` and `G` are 
    /// properly generated orthogonal base points (preventing hidden relationships).
    pub fn new(value: u64, blinding: Scalar) -> Self {
        let gens = PedersenGens::default();
        // Convert the u64 value to a Scalar
        let v_scalar = Scalar::from(value);
        Commitment(gens.commit(v_scalar, blinding))
    }
    
    /// Extract the inner RistrettoPoint
    pub fn as_point(&self) -> RistrettoPoint {
        self.0
    }
}

/// Homomorphic Addition: C(v1, r1) + C(v2, r2) = C(v1 + v2, r1 + r2)
impl Add for Commitment {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Commitment(self.0 + rhs.0)
    }
}

impl Sub for Commitment {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Commitment(self.0 - rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use curve25519_dalek::traits::Identity;
    
    #[test]
    fn test_homomorphic_addition() {
        let mut rng = OsRng;
        
        let v1 = 10u64;
        let r1 = Scalar::random(&mut rng);
        let c1 = Commitment::new(v1, r1);
        
        let v2 = 25u64;
        let r2 = Scalar::random(&mut rng);
        let c2 = Commitment::new(v2, r2);
        
        let sum_c = c1 + c2;
        let expected_c = Commitment::new(v1 + v2, r1 + r2);
        
        assert_eq!(sum_c.0, expected_c.0, "Homomorphic addition failed");
    }
    
    #[test]
    fn test_hiding_property() {
        let mut rng = OsRng;
        let v = 100u64;
        
        let r1 = Scalar::random(&mut rng);
        let r2 = Scalar::random(&mut rng);
        
        // Different blinding factors for the same value should result in different commitments
        assert_ne!(r1, r2);
        let c1 = Commitment::new(v, r1);
        let c2 = Commitment::new(v, r2);
        
        assert_ne!(c1.0, c2.0, "Hiding property failed: same value with different blinding yielded same commitment");
    }
    
    #[test]
    fn test_zero_sum_validation() {
        // In Mimblewimble, a valid transaction sum (inputs - outputs - fees) should commit to exactly 0 value.
        // The remaining commitment point must solely be `excess_blinding * G`.
        let mut rng = OsRng;
        
        // Inputs
        let in_v1 = 50u64;
        let in_r1 = Scalar::random(&mut rng);
        let in_c1 = Commitment::new(in_v1, in_r1);
        
        let in_v2 = 50u64;
        let in_r2 = Scalar::random(&mut rng);
        let in_c2 = Commitment::new(in_v2, in_r2);
        
        // Outputs
        let out_v1 = 90u64;
        let out_r1 = Scalar::random(&mut rng);
        let out_c1 = Commitment::new(out_v1, out_r1);
        
        let fee = 10u64; // implicitly committed to with zero blinding: fee * H + 0 * G
        let fee_c = Commitment::new(fee, Scalar::ZERO);
        
        // Net sum of commitments: (in_c1 + in_c2) - (out_c1 + fee_c)
        let sum_commitments = (in_c1 + in_c2) - (out_c1 + fee_c);
        
        // The excess blinding factor should be (in_r1 + in_r2) - out_r1 - 0
        let excess_blinding = in_r1 + in_r2 - out_r1;
        
        // Since values balance (50+50 - 90-10 = 0), the resulting commitment should purely be the blinding factor * G
        // So commit(0, excess_blinding)
        let expected_excess_commitment = Commitment::new(0, excess_blinding);
        
        assert_eq!(sum_commitments.0, expected_excess_commitment.0, "Zero sum validation failed");
    }
    
    #[test]
    fn test_binding_property() {
        // It should be hard to find a different value and blinding factor that yields the same commitment.
        let mut rng = OsRng;
        let v1 = 42u64;
        let r1 = Scalar::random(&mut rng);
        let c1 = Commitment::new(v1, r1);
        
        let v2 = 43u64;
        let c2 = Commitment::new(v2, r1);
        
        assert_ne!(c1.0, c2.0);
    }
}
