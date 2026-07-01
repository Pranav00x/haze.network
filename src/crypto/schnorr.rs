use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::scalar::Scalar;
use merlin::Transcript;
use rand::rngs::OsRng;

use super::pedersen::Commitment;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub s: Scalar,
    pub e: Scalar,
}

impl Signature {
    /// Sign a message using a secret key (which in Mimblewimble is the excess blinding factor).
    /// The public key is derived implicitly as `secret_key * G` which is what the kernel excess commits to.
    pub fn sign(message: &[u8], secret_key: &Scalar) -> Self {
        let mut rng = OsRng;
        let mut transcript = Transcript::new(b"Haze Schnorr Signature");
        
        // Add message to transcript
        transcript.append_message(b"message", message);
        
        // Generate a random nonce k
        let k = Scalar::random(&mut rng);
        let public_nonce = k * RISTRETTO_BASEPOINT_POINT;
        
        // Add public nonce to transcript to get challenge e
        transcript.append_message(b"public_nonce", public_nonce.compress().as_bytes());
        
        let mut e_bytes = [0u8; 64];
        transcript.challenge_bytes(b"e", &mut e_bytes);
        let e = Scalar::from_bytes_mod_order_wide(&e_bytes);
        
        // Compute s = k + e * secret_key
        let s = k + e * secret_key;
        
        Signature { s, e }
    }

    /// Verify a signature against a public key (which is a Commitment to 0 with excess blinding factor).
    pub fn verify(&self, message: &[u8], public_key: &Commitment) -> bool {
        let mut transcript = Transcript::new(b"Haze Schnorr Signature");
        
        // Add message to transcript
        transcript.append_message(b"message", message);
        
        // R = s * G - e * P
        let pk_point = public_key.as_point();
        let r_point = self.s * RISTRETTO_BASEPOINT_POINT - self.e * pk_point;
        
        // Recompute the challenge
        transcript.append_message(b"public_nonce", r_point.compress().as_bytes());
        
        let mut e_bytes = [0u8; 64];
        transcript.challenge_bytes(b"e", &mut e_bytes);
        let expected_e = Scalar::from_bytes_mod_order_wide(&e_bytes);
        
        self.e == expected_e
    }
}
