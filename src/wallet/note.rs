//! Recoverable output notes - a small piece of data attached to every
//! Output, encrypted under a key only this wallet's own seed can derive
//! (Keystore::note_key), containing exactly the two things a restored
//! wallet can't otherwise reconstruct: which index produced this output,
//! and what value it holds (a Pedersen commitment hides value by design).
//!
//! This is the same idea as Grin's rangeproof rewind, just implemented as
//! a standalone encrypted blob instead of data embedded in the bulletproof
//! itself (the `bulletproofs` crate used here has no rewind support). Since
//! every Output is always constructed by whoever owns it - self-pay,
//! two-party slate responses, faucet payouts - the creator always has their
//! own note_key on hand; no shared secret with a counterparty is needed.
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, aead::{Aead, KeyInit}};
use curve25519_dalek_ng::scalar::Scalar;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Sha512, Digest};

const PLAINTEXT_LEN: usize = 12; // u32 index + u64 value
const NONCE_LEN: usize = 12;

/// Deterministically derives a validator's coinbase blinding factor for a
/// given block height, from their staking secret alone - a real fix, not
/// just cosmetic: before this existed, the proposer generated a throwaway
/// random blinding for every coinbase output and discarded it immediately,
/// meaning nobody (not even the validator who "earned" it) could ever prove
/// ownership of a block reward. Every block's minted coins were
/// permanently unspendable. `stake_key` here is the same secret a validator
/// already holds (it's literally the blinding factor of whatever UTXO they
/// staked - see reveal_stake_blinding_hex), so this needs no new key
/// material, just a fixed point to derive from instead of randomness.
pub fn coinbase_blinding(stake_key: &Scalar, height: u64) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(b"Haze Coinbase Blinding");
    hasher.update(stake_key.as_bytes());
    hasher.update(&height.to_le_bytes());
    let result = hasher.finalize();
    let mut bytes = [0u8; 64];
    bytes.copy_from_slice(&result);
    Scalar::from_bytes_mod_order_wide(&bytes)
}

/// Note-encryption key for a validator's own coinbase rewards, derived from
/// their staking secret the same way Keystore::note_key derives one from a
/// wallet seed - lets a validator scan the chain for every block they
/// proposed and recover the reward value without any separate bookkeeping.
pub fn coinbase_note_key(stake_key: &Scalar) -> [u8; 32] {
    let mut hasher = Sha512::new();
    hasher.update(b"Haze Coinbase Note Key");
    hasher.update(stake_key.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result[0..32]);
    key
}

/// Encrypts (index, value) into a self-contained note (nonce || ciphertext).
pub fn seal(note_key: &[u8; 32], index: u32, value: u64) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(&Key::from(*note_key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);

    let mut plaintext = [0u8; PLAINTEXT_LEN];
    plaintext[0..4].copy_from_slice(&index.to_le_bytes());
    plaintext[4..12].copy_from_slice(&value.to_le_bytes());

    let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).expect("encryption with a fixed-size plaintext cannot fail");

    let mut note = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    note.extend_from_slice(&nonce_bytes);
    note.extend_from_slice(&ciphertext);
    note
}

/// Attempts to decrypt a note. Returns None if it wasn't sealed under this
/// note_key (wrong wallet's output) or is malformed - both look identical
/// from the outside, which is exactly the point: nobody but the owner can
/// tell a note apart from random bytes.
pub fn open(note_key: &[u8; 32], note: &[u8]) -> Option<(u32, u64)> {
    if note.len() < NONCE_LEN {
        return None;
    }
    let (nonce_bytes, ciphertext) = note.split_at(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(&Key::from(*note_key));
    let nonce = Nonce::try_from(nonce_bytes).ok()?;
    let plaintext = cipher.decrypt(&nonce, ciphertext).ok()?;
    if plaintext.len() != PLAINTEXT_LEN {
        return None;
    }
    let mut index_bytes = [0u8; 4];
    index_bytes.copy_from_slice(&plaintext[0..4]);
    let mut value_bytes = [0u8; 8];
    value_bytes.copy_from_slice(&plaintext[4..12]);
    Some((u32::from_le_bytes(index_bytes), u64::from_le_bytes(value_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_index_and_value() {
        let key = [7u8; 32];
        let note = seal(&key, 42, 123_456);
        assert_eq!(open(&key, &note), Some((42, 123_456)));
    }

    #[test]
    fn rejects_wrong_key() {
        let note = seal(&[1u8; 32], 1, 100);
        assert_eq!(open(&[2u8; 32], &note), None);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(open(&[1u8; 32], b"not a real note"), None);
    }

    #[test]
    fn coinbase_blinding_is_deterministic_and_height_separated() {
        let stake_key = Scalar::from(42u64);
        assert_eq!(coinbase_blinding(&stake_key, 5), coinbase_blinding(&stake_key, 5));
        assert_ne!(coinbase_blinding(&stake_key, 5), coinbase_blinding(&stake_key, 6));
        assert_ne!(coinbase_blinding(&stake_key, 5), coinbase_blinding(&Scalar::from(43u64), 5));
    }

    #[test]
    fn coinbase_note_key_round_trips_through_seal_open() {
        let stake_key = Scalar::from(99u64);
        let key = coinbase_note_key(&stake_key);
        let note = seal(&key, 10, 60);
        assert_eq!(open(&key, &note), Some((10, 60)));
    }
}
