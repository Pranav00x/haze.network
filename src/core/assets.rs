//! Haze Asset Registry - unique, non-fungible assets ("NFTs"), modeled
//! directly on core::registry's naming pattern: a permanent, first-mint-wins
//! asset_id -> record mapping committed into consensus state (see
//! BlockHeader::asset_registry_root), not a live-broadcast-only side
//! channel. Deliberately a SEPARATE namespace from .haze names - an asset_id
//! and a name are unrelated even if the strings collide, the same way an
//! ENS name and an NFT token ID don't share a space.
//!
//! Ownership, metadata, and every transfer here are fully public, by
//! design - same tier as names, not the confidential Pedersen/UTXO side of
//! the chain. There is no way to hide who owns a given asset; if that's
//! ever wanted, it's a fundamentally different (and much harder) problem
//! than this registry solves - see the multi-asset/NFT design doc.

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use curve25519_dalek_ng::scalar::Scalar;

use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use crate::core::transaction::Transaction;

/// Minimum one-time mint fee, paid as this op's fee_payment fee. A hard
/// consensus floor (validate_standalone runs at block-apply time, unlike the
/// payment mempool's MIN_FEE, which is mempool-acceptance policy only) -
/// same reasoning as core::registry::NAME_REGISTRATION_FEE: it has to be a
/// fixed floor, not a live congestion-derived value, since a hard
/// equality/threshold check must be deterministic for every validator. The
/// actual fee a wallet pays can be anything >= this floor - see
/// Mempool::suggested_asset_fee for the congestion-priced amount wallets
/// should actually offer.
pub const ASSET_MINT_FEE: u64 = 5;

pub const MIN_ASSET_ID_LENGTH: usize = 3;
pub const MAX_ASSET_ID_LENGTH: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetRecord {
    pub asset_id: String,
    pub owner_pubkey: Commitment,
    /// Commitment/pointer to off-chain metadata (image, attributes, etc.) -
    /// NFT metadata doesn't belong on-chain economically, same conclusion
    /// every real NFT ecosystem already lands on. Not interpreted by
    /// consensus at all, just carried along.
    pub metadata_hash: [u8; 32],
    pub minted_at_block: u64,
}

/// An asset mint, carried in a block alongside (not instead of) the normal
/// cut-through transaction. `fee_payment` is an ordinary Mimblewimble
/// transaction (inputs/outputs/one kernel) whose only job is to pay the mint
/// fee - reusing the existing balance-equation and fee-collection machinery
/// instead of inventing a second one. `signature` proves control of
/// `owner_pubkey` by signing the asset_id and metadata_hash together.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MintAssetOp {
    pub asset_id: String,
    pub owner_pubkey: Commitment,
    pub metadata_hash: [u8; 32],
    pub fee_payment: Transaction,
    pub signature: Signature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetError {
    TooShort,
    TooLong,
    InvalidCharacters,
    AlreadyMinted,
    InvalidSignature,
    InvalidFeePayment,
}

pub fn validate_asset_id(asset_id: &str) -> Result<(), AssetError> {
    if asset_id.len() < MIN_ASSET_ID_LENGTH {
        return Err(AssetError::TooShort);
    }
    if asset_id.len() > MAX_ASSET_ID_LENGTH {
        return Err(AssetError::TooLong);
    }
    if !asset_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err(AssetError::InvalidCharacters);
    }
    Ok(())
}

impl MintAssetOp {
    /// Builds the message signed to prove ownership of `owner_pubkey` - the
    /// asset_id and metadata_hash together, so a mint signature can't later
    /// be replayed to claim a different asset_id or with different metadata
    /// than what was actually signed.
    pub fn signing_message(asset_id: &str, metadata_hash: &[u8; 32]) -> Vec<u8> {
        let mut msg = asset_id.as_bytes().to_vec();
        msg.extend_from_slice(metadata_hash);
        msg
    }

    pub fn sign(asset_id: &str, metadata_hash: &[u8; 32], owner_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(asset_id, metadata_hash), owner_secret)
    }

    /// Validates this op in isolation (asset_id rules, signature, fee-payment's
    /// own internal balance/proof correctness). Does NOT check asset_id
    /// uniqueness or that fee_payment's inputs are real unspent UTXOs - those
    /// require chain state and are checked separately by the caller
    /// (ChainState::apply_linear_block) since they can only be verified there.
    pub fn validate_standalone(&self) -> Result<(), AssetError> {
        validate_asset_id(&self.asset_id)?;

        if !self.signature.verify(&Self::signing_message(&self.asset_id, &self.metadata_hash), &self.owner_pubkey) {
            return Err(AssetError::InvalidSignature);
        }

        if self.fee_payment.kernels.len() != 1 || self.fee_payment.kernels[0].fee < ASSET_MINT_FEE {
            return Err(AssetError::InvalidFeePayment);
        }
        if !self.fee_payment.validate() {
            return Err(AssetError::InvalidFeePayment);
        }

        Ok(())
    }
}

/// Hands ownership of an already-minted asset to a new owner. No fee, no
/// spendable UTXO involved - just a signature proving control of the
/// asset's *current* owner_pubkey, which only chain state knows (see
/// ChainState::apply_linear_block), so unlike MintAssetOp there's no useful
/// "validate_standalone" - every real check needs the current AssetRecord.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferAssetOp {
    pub asset_id: String,
    pub new_owner_pubkey: Commitment,
    /// Signed by the *current* owner's secret key - verified against
    /// whatever AssetRecord.owner_pubkey currently is, not anything in this
    /// struct, so a stale/forged transfer can't just supply its own key.
    pub signature: Signature,
}

impl TransferAssetOp {
    /// Domain-separated from MintAssetOp::signing_message (distinct prefix)
    /// so a mint signature can never be replayed as a transfer signature or
    /// vice versa, and binds in the new owner pubkey so a transfer can't be
    /// redirected to a different destination after the fact.
    pub fn signing_message(asset_id: &str, new_owner_pubkey: &Commitment) -> Vec<u8> {
        let mut msg = b"HazeAssetTransfer:".to_vec();
        msg.extend_from_slice(asset_id.as_bytes());
        msg.extend_from_slice(new_owner_pubkey.as_point().compress().as_bytes());
        msg
    }

    pub fn sign(asset_id: &str, new_owner_pubkey: &Commitment, current_owner_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(asset_id, new_owner_pubkey), current_owner_secret)
    }
}

/// A simple (not Merkle) commitment to the full asset registry state: sorted
/// by asset_id for determinism, then hashed - same approach and same
/// deferred-Merkle-proofs caveat as core::registry::compute_registry_root.
pub fn compute_asset_registry_root(registry: &HashMap<String, AssetRecord>) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut ids: Vec<&String> = registry.keys().collect();
    ids.sort();

    let mut hasher = Sha256::new();
    for id in ids {
        let record = &registry[id];
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
    fn accepts_valid_asset_ids() {
        assert!(validate_asset_id("cryptopunk").is_ok());
        assert!(validate_asset_id("abc").is_ok());
        assert!(validate_asset_id("a_b-c123").is_ok());
    }

    #[test]
    fn rejects_too_short() {
        assert_eq!(validate_asset_id("ab"), Err(AssetError::TooShort));
    }

    #[test]
    fn rejects_too_long() {
        let id = "a".repeat(MAX_ASSET_ID_LENGTH + 1);
        assert_eq!(validate_asset_id(&id), Err(AssetError::TooLong));
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(validate_asset_id("Punk"), Err(AssetError::InvalidCharacters));
        assert_eq!(validate_asset_id("pu nk"), Err(AssetError::InvalidCharacters));
        assert_eq!(validate_asset_id("pu@nk"), Err(AssetError::InvalidCharacters));
    }

    #[test]
    fn asset_registry_root_is_deterministic_and_order_independent() {
        let blinding_a = Scalar::from(1u64);
        let blinding_b = Scalar::from(2u64);
        let record_a = AssetRecord {
            asset_id: "alice-punk".to_string(),
            owner_pubkey: Commitment::new(0, blinding_a),
            metadata_hash: [1u8; 32],
            minted_at_block: 1,
        };
        let record_b = AssetRecord {
            asset_id: "bob-punk".to_string(),
            owner_pubkey: Commitment::new(0, blinding_b),
            metadata_hash: [2u8; 32],
            minted_at_block: 2,
        };

        let mut registry_1 = HashMap::new();
        registry_1.insert(record_a.asset_id.clone(), record_a.clone());
        registry_1.insert(record_b.asset_id.clone(), record_b.clone());

        let mut registry_2 = HashMap::new();
        registry_2.insert(record_b.asset_id.clone(), record_b.clone());
        registry_2.insert(record_a.asset_id.clone(), record_a.clone());

        assert_eq!(compute_asset_registry_root(&registry_1), compute_asset_registry_root(&registry_2));
        assert_ne!(compute_asset_registry_root(&registry_1), compute_asset_registry_root(&HashMap::new()));
    }

    #[test]
    fn signature_only_valid_for_its_own_asset_id_metadata_and_owner() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata_hash = [9u8; 32];

        let sig = MintAssetOp::sign("cryptopunk", &metadata_hash, &secret);
        assert!(sig.verify(&MintAssetOp::signing_message("cryptopunk", &metadata_hash), &owner_pubkey));
        assert!(!sig.verify(&MintAssetOp::signing_message("someoneelse", &metadata_hash), &owner_pubkey));
        assert!(!sig.verify(&MintAssetOp::signing_message("cryptopunk", &[0u8; 32]), &owner_pubkey));

        let other_secret = Scalar::from(8u64);
        let other_pubkey = Commitment(other_secret * gens.B_blinding);
        assert!(!sig.verify(&MintAssetOp::signing_message("cryptopunk", &metadata_hash), &other_pubkey));
    }

    #[test]
    fn transfer_signing_message_is_domain_separated_from_mint() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata_hash = [9u8; 32];

        let mint_sig = MintAssetOp::sign("cryptopunk", &metadata_hash, &secret);
        // A mint signature must never verify as a valid transfer signature
        // for the same asset_id, even targeting the same pubkey.
        assert!(!mint_sig.verify(&TransferAssetOp::signing_message("cryptopunk", &owner_pubkey), &owner_pubkey));
    }
}
