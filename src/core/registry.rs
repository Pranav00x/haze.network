//! Haze Naming Registry - a permanent, first-come-first-served name -> pubkey
//! mapping committed into consensus state (see BlockHeader::name_registry_root),
//! not a live-broadcast-only side channel like validator registration. That
//! distinction matters: a node that syncs blocks from scratch automatically
//! gets the full registry for free, the same way it gets the UTXO set, rather
//! than needing a separate catch-up mechanism (the gap that caused the P2P
//! validator-sync bug fixed elsewhere in this codebase).
//!
//! A name only ever resolves to a bare pubkey today - there's no transport
//! layer yet for a sender to actually reach that pubkey's holder to exchange
//! a slate (see the invoice flow / Tor listener, both future work).

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use curve25519_dalek_ng::scalar::Scalar;

use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use crate::core::transaction::Transaction;

/// One-time registration fee, paid as this transaction's fee (collected into
/// the block's coinbase like any other transaction fee - not silently burned).
pub const NAME_REGISTRATION_FEE: u64 = 5;

pub const MIN_NAME_LENGTH: usize = 3;
pub const MAX_NAME_LENGTH: usize = 32;

const RESERVED_NAMES: &[&str] = &[
    "haze", "admin", "team", "validator", "validators", "system", "root",
    "genesis", "faucet", "null", "none", "node", "api", "explorer", "wallet",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NameRecord {
    pub name: String,
    pub owner_pubkey: Commitment,
    pub resolves_to: Commitment,
    pub registered_at_block: u64,
}

/// A name registration, carried in a block alongside (not instead of) the
/// normal cut-through transaction. `fee_payment` is an ordinary Mimblewimble
/// transaction (inputs/outputs/one kernel) whose only job is to pay the
/// registration fee - reusing the existing balance-equation and fee-collection
/// machinery instead of inventing a second one. `signature` proves control of
/// `owner_pubkey` by signing the name bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterNameOp {
    pub name: String,
    pub owner_pubkey: Commitment,
    pub resolves_to: Commitment,
    pub fee_payment: Transaction,
    pub signature: Signature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    TooShort,
    TooLong,
    InvalidCharacters,
    Reserved,
    AlreadyRegistered,
    InvalidSignature,
    InvalidFeePayment,
}

pub fn validate_name(name: &str) -> Result<(), NameError> {
    if name.len() < MIN_NAME_LENGTH {
        return Err(NameError::TooShort);
    }
    if name.len() > MAX_NAME_LENGTH {
        return Err(NameError::TooLong);
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err(NameError::InvalidCharacters);
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(NameError::Reserved);
    }
    Ok(())
}

impl RegisterNameOp {
    /// Builds the message signed to prove ownership of `owner_pubkey` - just
    /// the name bytes, since (unlike a payment kernel) there's no fee/amount
    /// that also needs binding into the signature.
    pub fn signing_message(name: &str) -> Vec<u8> {
        name.as_bytes().to_vec()
    }

    pub fn sign(name: &str, owner_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(name), owner_secret)
    }

    /// Validates this op in isolation (name rules, signature, fee-payment's
    /// own internal balance/proof correctness). Does NOT check name
    /// uniqueness or that fee_payment's inputs are real unspent UTXOs -
    /// those require chain state and are checked separately by the caller
    /// (ChainState::apply_name_op) since they can only be verified there.
    pub fn validate_standalone(&self) -> Result<(), NameError> {
        validate_name(&self.name)?;

        if !self.signature.verify(&Self::signing_message(&self.name), &self.owner_pubkey) {
            return Err(NameError::InvalidSignature);
        }

        if self.fee_payment.kernels.len() != 1 || self.fee_payment.kernels[0].fee != NAME_REGISTRATION_FEE {
            return Err(NameError::InvalidFeePayment);
        }
        if !self.fee_payment.validate() {
            return Err(NameError::InvalidFeePayment);
        }

        Ok(())
    }
}

/// Hands ownership of an already-registered name to a new owner/resolution
/// target. No fee, no spendable UTXO involved - just a signature proving
/// control of the name's *current* owner_pubkey, which only chain state
/// knows (see ChainState::apply_linear_block), so unlike RegisterNameOp
/// there's no useful "validate_standalone" - every real check needs the
/// current NameRecord.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferNameOp {
    pub name: String,
    pub new_owner_pubkey: Commitment,
    pub new_resolves_to: Commitment,
    /// Signed by the *current* owner's secret key - verified against
    /// whatever NameRecord.owner_pubkey currently is, not anything in this
    /// struct, so a stale/forged transfer can't just supply its own key.
    pub signature: Signature,
}

impl TransferNameOp {
    /// Domain-separated from RegisterNameOp::signing_message (distinct
    /// prefix) so a registration signature can never be replayed as a
    /// transfer signature or vice versa, and binds in the new owner/resolves
    /// pubkeys so a transfer can't be redirected to a different destination
    /// after the fact.
    pub fn signing_message(name: &str, new_owner_pubkey: &Commitment, new_resolves_to: &Commitment) -> Vec<u8> {
        let mut msg = b"HazeNameTransfer:".to_vec();
        msg.extend_from_slice(name.as_bytes());
        msg.extend_from_slice(new_owner_pubkey.as_point().compress().as_bytes());
        msg.extend_from_slice(new_resolves_to.as_point().compress().as_bytes());
        msg
    }

    pub fn sign(name: &str, new_owner_pubkey: &Commitment, new_resolves_to: &Commitment, current_owner_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(name, new_owner_pubkey, new_resolves_to), current_owner_secret)
    }
}

/// A simple (not Merkle) commitment to the full registry state: sorted by
/// name for determinism, then hashed. Enough for every node to verify a
/// block's claimed registry state matches what they compute themselves from
/// applying the same name ops - real Merkle proofs (for light clients that
/// don't want to hold the whole registry) are explicitly deferred, per the
/// build spec's open questions.
pub fn compute_registry_root(registry: &HashMap<String, NameRecord>) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut names: Vec<&String> = registry.keys().collect();
    names.sort();

    let mut hasher = Sha256::new();
    for name in names {
        let record = &registry[name];
        let encoded = bincode::serialize(record).unwrap();
        hasher.update(&encoded);
    }
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        assert!(validate_name("pranav").is_ok());
        assert!(validate_name("abc").is_ok());
        assert!(validate_name("a_b-c123").is_ok());
    }

    #[test]
    fn rejects_too_short() {
        assert_eq!(validate_name("ab"), Err(NameError::TooShort));
    }

    #[test]
    fn rejects_too_long() {
        let name = "a".repeat(MAX_NAME_LENGTH + 1);
        assert_eq!(validate_name(&name), Err(NameError::TooLong));
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(validate_name("Pranav"), Err(NameError::InvalidCharacters));
        assert_eq!(validate_name("pra nav"), Err(NameError::InvalidCharacters));
        assert_eq!(validate_name("pra@nav"), Err(NameError::InvalidCharacters));
    }

    #[test]
    fn rejects_reserved_names() {
        assert_eq!(validate_name("haze"), Err(NameError::Reserved));
        assert_eq!(validate_name("admin"), Err(NameError::Reserved));
        assert_eq!(validate_name("validator"), Err(NameError::Reserved));
    }

    #[test]
    fn registry_root_is_deterministic_and_order_independent() {
        let blinding_a = Scalar::from(1u64);
        let blinding_b = Scalar::from(2u64);
        let record_a = NameRecord {
            name: "alice".to_string(),
            owner_pubkey: Commitment::new(0, blinding_a),
            resolves_to: Commitment::new(0, blinding_a),
            registered_at_block: 1,
        };
        let record_b = NameRecord {
            name: "bob".to_string(),
            owner_pubkey: Commitment::new(0, blinding_b),
            resolves_to: Commitment::new(0, blinding_b),
            registered_at_block: 2,
        };

        let mut registry_1 = HashMap::new();
        registry_1.insert(record_a.name.clone(), record_a.clone());
        registry_1.insert(record_b.name.clone(), record_b.clone());

        let mut registry_2 = HashMap::new();
        registry_2.insert(record_b.name.clone(), record_b.clone());
        registry_2.insert(record_a.name.clone(), record_a.clone());

        assert_eq!(compute_registry_root(&registry_1), compute_registry_root(&registry_2));
        assert_ne!(compute_registry_root(&registry_1), compute_registry_root(&HashMap::new()));
    }

    #[test]
    fn signature_only_valid_for_its_own_name_and_owner() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);

        let sig = RegisterNameOp::sign("pranav", &secret);
        assert!(sig.verify(&RegisterNameOp::signing_message("pranav"), &owner_pubkey));
        assert!(!sig.verify(&RegisterNameOp::signing_message("someoneelse"), &owner_pubkey));

        let other_secret = Scalar::from(8u64);
        let other_pubkey = Commitment(other_secret * gens.B_blinding);
        assert!(!sig.verify(&RegisterNameOp::signing_message("pranav"), &other_pubkey));
    }
}
