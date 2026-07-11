use curve25519_dalek_ng::scalar::Scalar;
use merlin::Transcript;
use rand::rngs::OsRng;
use bulletproofs::PedersenGens;


use super::pedersen::Commitment;
use serde::{Serialize, Serializer, Deserialize, Deserializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub s: Scalar,
    pub e: Scalar,
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = [0u8; 64];
        bytes[0..32].copy_from_slice(self.s.as_bytes());
        bytes[32..64].copy_from_slice(self.e.as_bytes());
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("Invalid signature length"));
        }
        
        let mut s_bytes = [0u8; 32];
        s_bytes.copy_from_slice(&bytes[0..32]);
        let s = curve25519_dalek_ng::scalar::Scalar::from_canonical_bytes(s_bytes)
            .ok_or_else(|| serde::de::Error::custom("Invalid scalar s"))?;
            
        let mut e_bytes = [0u8; 32];
        e_bytes.copy_from_slice(&bytes[32..64]);
        let e = curve25519_dalek_ng::scalar::Scalar::from_canonical_bytes(e_bytes)
            .ok_or_else(|| serde::de::Error::custom("Invalid scalar e"))?;
            
        Ok(Signature { s, e })
    }
}

impl Signature {
    /// Sign a message using a secret key (which in Mimblewimble is the excess blinding factor).
    /// The public key is derived implicitly as `secret_key * H` which is what the kernel excess commits to.
    pub fn sign(message: &[u8], secret_key: &Scalar) -> Self {
        let mut rng = OsRng;
        let mut transcript = Transcript::new(b"Haze Schnorr Signature");
        let gens = PedersenGens::default();
        
        // Derive public key point
        let public_key = secret_key * gens.B_blinding;
        
        // Add message and public key to transcript
        transcript.append_message(b"message", message);
        transcript.append_message(b"public_key", public_key.compress().as_bytes());
        
        // Generate a random nonce k
        let k = Scalar::random(&mut rng);
        let public_nonce = k * gens.B_blinding;
        
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
        let gens = PedersenGens::default();
        
        let pk_point = public_key.as_point();
        
        // Add message and public key to transcript
        transcript.append_message(b"message", message);
        transcript.append_message(b"public_key", pk_point.compress().as_bytes());
        
        // R = s * H - e * P
        let r_point = self.s * gens.B_blinding - self.e * pk_point;
        
        // Recompute the challenge
        transcript.append_message(b"public_nonce", r_point.compress().as_bytes());
        
        let mut e_bytes = [0u8; 64];
        transcript.challenge_bytes(b"e", &mut e_bytes);
        let expected_e = Scalar::from_bytes_mod_order_wide(&e_bytes);
        
        self.e == expected_e
    }

    /// Encodes the signature as a lowercase hex string (s || e, 64 bytes) -
    /// mirrors Commitment::to_hex, used wherever a signature needs to travel
    /// as a compact string rather than JSON (e.g. wasm bindings for the
    /// marketplace connect-wallet handoff).
    pub fn to_hex(&self) -> String {
        let mut bytes = [0u8; 64];
        bytes[0..32].copy_from_slice(self.s.as_bytes());
        bytes[32..64].copy_from_slice(self.e.as_bytes());
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Parses a signature from a lowercase hex string produced by to_hex().
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 128 {
            return None;
        }
        let mut bytes = [0u8; 64];
        for i in 0..64 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        let mut s_bytes = [0u8; 32];
        s_bytes.copy_from_slice(&bytes[0..32]);
        let s = Scalar::from_canonical_bytes(s_bytes)?;
        let mut e_bytes = [0u8; 32];
        e_bytes.copy_from_slice(&bytes[32..64]);
        let e = Scalar::from_canonical_bytes(e_bytes)?;
        Some(Signature { s, e })
    }
}

/// Additive support for two-party (interactive) Schnorr signing, e.g. for the
/// slate-exchange payment protocol (src/wallet/slate.rs). These functions are
/// deliberately separate from sign()/verify() above rather than refactoring
/// them - the existing single-party path is already tested and used by every
/// kernel construction site (genesis, coinbase, self-pay); this just adds a
/// second, additive way to arrive at a signature that satisfies the same
/// unmodified verify().
///
/// A two-party signature is built as: each side picks a secret nonce and
/// publishes its point, the points are summed to get the aggregate nonce, a
/// shared challenge `e` is derived from (message, aggregate public key,
/// aggregate nonce) - using the exact same transcript order as sign()/verify()
/// - and each side computes a partial signature `nonce + e * secret_key`.
/// Summing the partial signatures yields a valid (s, e) pair for the
/// aggregate public key.
use curve25519_dalek_ng::ristretto::RistrettoPoint;

/// Generates a random nonce and its public point (nonce * B_blinding).
pub fn generate_nonce() -> (Scalar, RistrettoPoint) {
    let mut rng = OsRng;
    let gens = PedersenGens::default();
    let k = Scalar::random(&mut rng);
    (k, k * gens.B_blinding)
}

/// Computes the shared Schnorr challenge for a message, an aggregate public
/// key point, and an aggregate nonce point - using the same transcript
/// convention as sign()/verify(), so a signature assembled from partial
/// contributions using this `e` satisfies the existing verify() unchanged.
pub fn compute_challenge(message: &[u8], public_key_point: RistrettoPoint, nonce_point: RistrettoPoint) -> Scalar {
    let mut transcript = Transcript::new(b"Haze Schnorr Signature");
    transcript.append_message(b"message", message);
    transcript.append_message(b"public_key", public_key_point.compress().as_bytes());
    transcript.append_message(b"public_nonce", nonce_point.compress().as_bytes());

    let mut e_bytes = [0u8; 64];
    transcript.challenge_bytes(b"e", &mut e_bytes);
    Scalar::from_bytes_mod_order_wide(&e_bytes)
}

/// Computes one party's partial signature scalar given the shared challenge.
pub fn partial_sign(nonce: &Scalar, secret_key: &Scalar, e: &Scalar) -> Scalar {
    nonce + e * secret_key
}

/// Combines partial signature scalars (and the shared challenge) into a final Signature.
pub fn aggregate(partial_sigs: &[Scalar], e: Scalar) -> Signature {
    let s: Scalar = partial_sigs.iter().sum();
    Signature { s, e }
}
