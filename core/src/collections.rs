//! Collection launches with scheduled, multi-phase minting (GTD allowlist /
//! FCFS allowlist / Public rounds - the standard NFT "drop" mechanic),
//! modeled directly on core::assets/core::registry's pattern: a permanent,
//! first-launch-wins collection_id -> record mapping committed into
//! consensus state (see BlockHeader::collection_registry_root), not a
//! live-broadcast-only side channel. A collection's phase schedule and
//! allowlist roots must be consensus (not gossip like core::marketplace's
//! Listing) because eligibility/timing has to be the same tamper-proof rule
//! every node agrees on - unlike a marketplace listing, which is just an
//! advertisement nobody needs to trust.
//!
//! The actual per-phase allowlist (the full plaintext pubkey list a Merkle
//! root commits to) is deliberately NOT stored here - only the root is
//! consensus. The list itself is off-chain/gossiped, see core::allowlist,
//! mirroring how core::marketplace keeps listings out of blocks entirely.

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use curve25519_dalek_ng::scalar::Scalar;

use haze_crypto::pedersen::Commitment;
use haze_crypto::schnorr::Signature;
use crate::assets::MAX_METADATA_BYTES;

pub const MIN_COLLECTION_ID_LENGTH: usize = 3;
pub const MAX_COLLECTION_ID_LENGTH: usize = 64;
pub const MAX_COLLECTION_NAME_LENGTH: usize = 64;
pub const MAX_COLLECTION_SYMBOL_LENGTH: usize = 16;
/// Basis points denominator (10000 = 100%) - caps royalty_bps so a creator
/// can't demand more than the entire resale price.
pub const MAX_ROYALTY_BPS: u16 = 10000;
/// Far more than any real drop needs (a GTD/FCFS/Public schedule is 3) -
/// caps phases.len() so a launch (which has no fee_payment, costing nothing
/// beyond ordinary block inclusion) can't be used to bloat every node's
/// permanent collection_registry state for free.
pub const MAX_PHASES: usize = 32;

/// One round of a collection's mint schedule - e.g. a "GTD" allowlist round,
/// an "FCFS" allowlist round, or an open "Public" round (allowlist_merkle_root
/// = None). Phases are validated (see LaunchCollectionOp::validate_standalone)
/// to be strictly sequential and non-overlapping, so "the active phase at
/// timestamp T" is always unambiguous - at most one phase can ever satisfy
/// start_time <= T < end_time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MintPhase {
    pub name: String,
    pub start_time: u64,
    pub end_time: u64,
    pub price: u64,
    pub per_wallet_limit: u32,
    /// None = open to anyone (a "Public" round, no proof required). Some(root)
    /// = only pubkeys whose leaf is provably included under this Merkle root
    /// (see core::merkle) may mint in this phase - the root is consensus, the
    /// underlying plaintext list is not (see core::allowlist).
    pub allowlist_merkle_root: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionRecord {
    pub collection_id: String,
    pub creator_pubkey: Commitment,
    pub name: String,
    pub symbol: String,
    pub metadata: Vec<u8>,
    pub phases: Vec<MintPhase>,
    pub launched_at_block: u64,
    /// Basis points (0-MAX_ROYALTY_BPS) of every secondary-sale price paid
    /// to creator_pubkey on top of the seller's proceeds - see
    /// TransferAssetOp::required_royalty_kernel_excess for how this is
    /// enforced trustlessly at consensus. Fixed for the collection's entire
    /// lifetime (set once at launch, never editable) - a buyer computing
    /// the royalty split needs a value that can't change out from under a
    /// transfer they've already conditioned their payment on.
    pub royalty_bps: u16,
}

/// A collection launch, carried in a block alongside (not instead of) the
/// normal cut-through transaction. Unlike MintAssetOp/RegisterNameOp, this
/// has no fee_payment - launching costs nothing beyond the ordinary
/// block-inclusion/gossip cost, since the real economic activity (and the
/// only thing worth spam-limiting) is the mints against it, not the launch
/// itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchCollectionOp {
    pub collection_id: String,
    pub creator_pubkey: Commitment,
    pub name: String,
    pub symbol: String,
    pub metadata: Vec<u8>,
    pub phases: Vec<MintPhase>,
    pub royalty_bps: u16,
    pub signature: Signature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionError {
    TooShort,
    TooLong,
    InvalidCharacters,
    AlreadyLaunched,
    InvalidSignature,
    MetadataTooLong,
    NameTooLong,
    SymbolTooLong,
    NoPhases,
    TooManyPhases,
    InvalidPhaseWindow,
    PhasesNotSequential,
    ZeroPerWalletLimit,
    RoyaltyTooHigh,
}

pub fn validate_collection_id(collection_id: &str) -> Result<(), CollectionError> {
    if collection_id.len() < MIN_COLLECTION_ID_LENGTH {
        return Err(CollectionError::TooShort);
    }
    if collection_id.len() > MAX_COLLECTION_ID_LENGTH {
        return Err(CollectionError::TooLong);
    }
    if !collection_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err(CollectionError::InvalidCharacters);
    }
    Ok(())
}

/// Phases must be non-empty, each with start_time < end_time and a non-zero
/// per_wallet_limit, and strictly sequential/non-overlapping (phase[i].end_time
/// <= phase[i+1].start_time) - this is what makes "the currently active
/// phase" well-defined at block-apply time.
fn validate_phases(phases: &[MintPhase]) -> Result<(), CollectionError> {
    if phases.is_empty() {
        return Err(CollectionError::NoPhases);
    }
    if phases.len() > MAX_PHASES {
        return Err(CollectionError::TooManyPhases);
    }
    for phase in phases {
        if phase.name.len() > MAX_COLLECTION_NAME_LENGTH {
            return Err(CollectionError::NameTooLong);
        }
        if phase.start_time >= phase.end_time {
            return Err(CollectionError::InvalidPhaseWindow);
        }
        if phase.per_wallet_limit == 0 {
            return Err(CollectionError::ZeroPerWalletLimit);
        }
    }
    for pair in phases.windows(2) {
        if pair[0].end_time > pair[1].start_time {
            return Err(CollectionError::PhasesNotSequential);
        }
    }
    Ok(())
}

impl LaunchCollectionOp {
    /// Binds every field a creator commits to at launch time, including the
    /// full phase schedule (bincode-serialized into the message) - so a
    /// launch signature can't later be replayed with different phases,
    /// pricing, or allowlist roots than what was actually signed. Distinct
    /// domain-separation prefix from MintAssetOp/TransferAssetOp so a launch
    /// signature can never be replayed as a mint/transfer signature.
    pub fn signing_message(collection_id: &str, creator_pubkey: &Commitment, name: &str, symbol: &str, metadata: &[u8], phases: &[MintPhase], royalty_bps: u16) -> Vec<u8> {
        let mut msg = b"HazeLaunchCollection:".to_vec();
        msg.extend_from_slice(collection_id.as_bytes());
        msg.extend_from_slice(creator_pubkey.as_point().compress().as_bytes());
        msg.extend_from_slice(name.as_bytes());
        msg.extend_from_slice(symbol.as_bytes());
        msg.extend_from_slice(metadata);
        msg.extend_from_slice(&bincode::serialize(phases).expect("phases serialize"));
        msg.extend_from_slice(&royalty_bps.to_le_bytes());
        msg
    }

    pub fn sign(collection_id: &str, creator_pubkey: &Commitment, name: &str, symbol: &str, metadata: &[u8], phases: &[MintPhase], royalty_bps: u16, creator_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(collection_id, creator_pubkey, name, symbol, metadata, phases, royalty_bps), creator_secret)
    }

    /// Validates this op in isolation (id/name/symbol/metadata shape, phase
    /// sanity, signature). Does NOT check collection_id uniqueness - that
    /// requires chain state and is checked separately by the caller
    /// (ChainState::apply_linear_block), same convention as
    /// MintAssetOp::validate_standalone.
    pub fn validate_standalone(&self) -> Result<(), CollectionError> {
        validate_collection_id(&self.collection_id)?;

        if self.name.len() > MAX_COLLECTION_NAME_LENGTH {
            return Err(CollectionError::NameTooLong);
        }
        if self.symbol.len() > MAX_COLLECTION_SYMBOL_LENGTH {
            return Err(CollectionError::SymbolTooLong);
        }
        if self.metadata.len() > MAX_METADATA_BYTES {
            return Err(CollectionError::MetadataTooLong);
        }
        if self.royalty_bps > MAX_ROYALTY_BPS {
            return Err(CollectionError::RoyaltyTooHigh);
        }
        validate_phases(&self.phases)?;

        let msg = Self::signing_message(&self.collection_id, &self.creator_pubkey, &self.name, &self.symbol, &self.metadata, &self.phases, self.royalty_bps);
        if !self.signature.verify(&msg, &self.creator_pubkey) {
            return Err(CollectionError::InvalidSignature);
        }

        Ok(())
    }
}

/// Canonical hash of a pubkey used as a Merkle leaf in an allowlist tree
/// (see core::merkle, core::allowlist) - both the off-chain proof-builder
/// (client-side JS/WASM) and the on-chain verifier (ChainState::apply_linear_block)
/// must use this exact same function, or a legitimately-allowlisted pubkey's
/// proof will never verify.
pub fn allowlist_leaf(pubkey: &Commitment) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(pubkey.as_point().compress().as_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Mirrors core::assets::compute_asset_registry_root exactly: sorted by
/// collection_id for determinism, then hashed.
pub fn compute_collection_registry_root(registry: &HashMap<String, CollectionRecord>) -> [u8; 32] {
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

    fn phase(name: &str, start: u64, end: u64, price: u64, limit: u32, root: Option<[u8; 32]>) -> MintPhase {
        MintPhase { name: name.to_string(), start_time: start, end_time: end, price, per_wallet_limit: limit, allowlist_merkle_root: root }
    }

    fn signed_op(phases: Vec<MintPhase>) -> (LaunchCollectionOp, Commitment) {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let creator_pubkey = Commitment(secret * gens.B_blinding);
        let signature = LaunchCollectionOp::sign("cryptopunks", &creator_pubkey, "CryptoPunks", "PUNK", b"a collection", &phases, 500, &secret);
        (LaunchCollectionOp { collection_id: "cryptopunks".to_string(), creator_pubkey, name: "CryptoPunks".to_string(), symbol: "PUNK".to_string(), metadata: b"a collection".to_vec(), phases, royalty_bps: 500, signature }, creator_pubkey)
    }

    #[test]
    fn accepts_valid_collection_ids() {
        assert!(validate_collection_id("cryptopunks").is_ok());
        assert!(validate_collection_id("abc").is_ok());
        assert!(validate_collection_id("a_b-c123").is_ok());
    }

    #[test]
    fn rejects_too_short_and_too_long_and_bad_chars() {
        assert_eq!(validate_collection_id("ab"), Err(CollectionError::TooShort));
        let id = "a".repeat(MAX_COLLECTION_ID_LENGTH + 1);
        assert_eq!(validate_collection_id(&id), Err(CollectionError::TooLong));
        assert_eq!(validate_collection_id("Punks"), Err(CollectionError::InvalidCharacters));
    }

    #[test]
    fn valid_sequential_phases_are_accepted() {
        let phases = vec![
            phase("GTD", 100, 200, 10, 1, Some([1u8; 32])),
            phase("FCFS", 200, 300, 15, 2, Some([2u8; 32])),
            phase("Public", 300, 400, 20, 5, None),
        ];
        let (op, _) = signed_op(phases);
        assert!(op.validate_standalone().is_ok());
    }

    #[test]
    fn rejects_empty_phases() {
        let (op, _) = signed_op(vec![]);
        assert_eq!(op.validate_standalone(), Err(CollectionError::NoPhases));
    }

    #[test]
    fn rejects_too_many_phases() {
        let phases: Vec<MintPhase> = (0..(MAX_PHASES as u64 + 1))
            .map(|i| phase("P", i * 100, i * 100 + 50, 1, 1, None))
            .collect();
        let (op, _) = signed_op(phases);
        assert_eq!(op.validate_standalone(), Err(CollectionError::TooManyPhases));
    }

    #[test]
    fn rejects_an_oversized_phase_name() {
        let long_name = "x".repeat(MAX_COLLECTION_NAME_LENGTH + 1);
        let phases = vec![phase(&long_name, 100, 200, 10, 1, None)];
        let (op, _) = signed_op(phases);
        assert_eq!(op.validate_standalone(), Err(CollectionError::NameTooLong));
    }

    #[test]
    fn rejects_inverted_phase_window() {
        let phases = vec![phase("GTD", 200, 100, 10, 1, None)];
        let (op, _) = signed_op(phases);
        assert_eq!(op.validate_standalone(), Err(CollectionError::InvalidPhaseWindow));
    }

    #[test]
    fn rejects_overlapping_phases() {
        let phases = vec![
            phase("GTD", 100, 250, 10, 1, None),
            phase("FCFS", 200, 300, 15, 2, None),
        ];
        let (op, _) = signed_op(phases);
        assert_eq!(op.validate_standalone(), Err(CollectionError::PhasesNotSequential));
    }

    #[test]
    fn rejects_zero_per_wallet_limit() {
        let phases = vec![phase("GTD", 100, 200, 10, 0, None)];
        let (op, _) = signed_op(phases);
        assert_eq!(op.validate_standalone(), Err(CollectionError::ZeroPerWalletLimit));
    }

    #[test]
    fn rejects_royalty_above_max_bps() {
        let phases = vec![phase("GTD", 100, 200, 10, 1, None)];
        let (mut op, _) = signed_op(phases);
        op.royalty_bps = MAX_ROYALTY_BPS + 1;
        assert_eq!(op.validate_standalone(), Err(CollectionError::RoyaltyTooHigh));
    }

    #[test]
    fn tampering_royalty_bps_invalidates_signature() {
        let phases = vec![phase("GTD", 100, 200, 10, 1, None)];
        let (mut op, _) = signed_op(phases);
        op.royalty_bps = 250;
        assert_eq!(op.validate_standalone(), Err(CollectionError::InvalidSignature));
    }

    /// Mirrors MintAssetOp/TransferAssetOp's load-bearing binding tests:
    /// tampering any signed field (here, any phase field) must invalidate
    /// the signature.
    #[test]
    fn tampering_any_phase_field_invalidates_signature() {
        let phases = vec![phase("GTD", 100, 200, 10, 1, None)];
        let (mut op, _) = signed_op(phases);
        op.phases[0].price = 999;
        assert_eq!(op.validate_standalone(), Err(CollectionError::InvalidSignature));
    }

    #[test]
    fn tampering_name_or_symbol_or_metadata_invalidates_signature() {
        let phases = vec![phase("GTD", 100, 200, 10, 1, None)];
        let (op, _) = signed_op(phases);

        let mut tampered_name = op.clone();
        tampered_name.name = "Different".to_string();
        assert_eq!(tampered_name.validate_standalone(), Err(CollectionError::InvalidSignature));

        let mut tampered_symbol = op.clone();
        tampered_symbol.symbol = "XXX".to_string();
        assert_eq!(tampered_symbol.validate_standalone(), Err(CollectionError::InvalidSignature));

        let mut tampered_metadata = op.clone();
        tampered_metadata.metadata = b"different".to_vec();
        assert_eq!(tampered_metadata.validate_standalone(), Err(CollectionError::InvalidSignature));
    }

    #[test]
    fn signing_message_is_domain_separated_from_mint_and_transfer() {
        use crate::assets::{MintAssetOp, TransferAssetOp};
        let phases = vec![phase("GTD", 100, 200, 10, 1, None)];
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let creator_pubkey = Commitment(secret * gens.B_blinding);

        let launch_sig = LaunchCollectionOp::sign("cryptopunks", &creator_pubkey, "CryptoPunks", "PUNK", b"meta", &phases, 500, &secret);
        // A launch signature must never verify as a mint or transfer signature.
        assert!(!launch_sig.verify(&MintAssetOp::signing_message("cryptopunks", b"meta", &None, &None, &None), &creator_pubkey));
        assert!(!launch_sig.verify(&TransferAssetOp::signing_message("cryptopunks", &creator_pubkey, &None, &None), &creator_pubkey));
    }

    #[test]
    fn collection_registry_root_is_deterministic_and_order_independent() {
        let (op_a, creator_a) = signed_op(vec![phase("GTD", 100, 200, 10, 1, None)]);
        let record_a = CollectionRecord {
            collection_id: op_a.collection_id.clone(),
            creator_pubkey: creator_a,
            name: op_a.name.clone(),
            symbol: op_a.symbol.clone(),
            metadata: op_a.metadata.clone(),
            phases: op_a.phases.clone(),
            launched_at_block: 1,
            royalty_bps: op_a.royalty_bps,
        };
        let secret_b = Scalar::from(9u64);
        let gens = bulletproofs::PedersenGens::default();
        let creator_b = Commitment(secret_b * gens.B_blinding);
        let record_b = CollectionRecord {
            collection_id: "another".to_string(),
            creator_pubkey: creator_b,
            name: "Another".to_string(),
            symbol: "ANO".to_string(),
            metadata: vec![],
            phases: vec![phase("Public", 0, 1000, 5, 10, None)],
            launched_at_block: 2,
            royalty_bps: 0,
        };

        let mut registry_1 = HashMap::new();
        registry_1.insert(record_a.collection_id.clone(), record_a.clone());
        registry_1.insert(record_b.collection_id.clone(), record_b.clone());

        let mut registry_2 = HashMap::new();
        registry_2.insert(record_b.collection_id.clone(), record_b.clone());
        registry_2.insert(record_a.collection_id.clone(), record_a.clone());

        assert_eq!(compute_collection_registry_root(&registry_1), compute_collection_registry_root(&registry_2));
        assert_ne!(compute_collection_registry_root(&registry_1), compute_collection_registry_root(&HashMap::new()));
    }
}
