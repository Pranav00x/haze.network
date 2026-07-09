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

/// Raw metadata (recommended shape: JSON `{title, description, image}`, but
/// consensus only enforces the byte-length cap, not the interpretation -
/// keeps the schema upgradeable without another chain reset) is stored
/// directly on-chain rather than just a hash of it. A browsable marketplace
/// needs real preview data; hash-only storage would make that depend on an
/// external metadata host, reintroducing exactly the kind of "trust someone
/// else's server" dependency the trustless-swap design is trying to
/// eliminate. At this cap and MAX_MINT_OPS_PER_BLOCK, worst case is a
/// trivial ~20KB/block of extra data for a chain that already keeps full
/// history forever.
pub const MAX_METADATA_BYTES: usize = 2048;
/// A real Merkle proof's length is the allowlist's log2(leaf count) - even a
/// billion-entry allowlist needs at most ~30 sibling hashes. 64 is generous
/// headroom while still ruling out a spam MintAssetOp padding this field to
/// waste verification time/bandwidth for no real proof-of-membership
/// purpose (see core::merkle::verify_merkle_proof, which has no length cap
/// of its own since it's a pure, general-purpose primitive).
pub const MAX_ALLOWLIST_PROOF_LENGTH: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetRecord {
    pub asset_id: String,
    pub owner_pubkey: Commitment,
    /// Raw metadata bytes (see MAX_METADATA_BYTES) - not interpreted by
    /// consensus at all beyond the length cap, just carried along.
    pub metadata: Vec<u8>,
    pub minted_at_block: u64,
    /// Which collection this asset was minted from, if any - carried
    /// forward from MintAssetOp.collection_id at mint time and never
    /// changed afterward, so a later resale can look up the collection's
    /// royalty_bps/creator_pubkey (see core::collections::CollectionRecord,
    /// TransferAssetOp::required_royalty_kernel_excess) regardless of how
    /// many times the asset has changed hands since.
    pub collection_id: Option<String>,
}

/// An asset mint, carried in a block alongside (not instead of) the normal
/// cut-through transaction. `fee_payment` is an ordinary Mimblewimble
/// transaction (inputs/outputs/one kernel) whose only job is to pay the mint
/// fee - reusing the existing balance-equation and fee-collection machinery
/// instead of inventing a second one. `signature` proves control of
/// `owner_pubkey` by signing the asset_id and metadata together (plus
/// collection_id/phase_index/required_kernel_excess when this is a
/// collection-drop mint - see MintAssetOp::signing_message).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MintAssetOp {
    pub asset_id: String,
    pub owner_pubkey: Commitment,
    pub metadata: Vec<u8>,
    pub fee_payment: Transaction,
    /// Which collection this mint is claimed against, if any - see
    /// core::collections. None for an ordinary standalone mint (unchanged
    /// behavior from before collection drops existed).
    pub collection_id: Option<String>,
    /// Index into the collection's phases array - which round (GTD/FCFS/
    /// Public) this mint claims to be eligible under. Must be Some iff
    /// collection_id is Some (checked in validate_standalone).
    pub phase_index: Option<u32>,
    /// Merkle inclusion proof of owner_pubkey under the claimed phase's
    /// allowlist_merkle_root (see core::merkle) - only required (and only
    /// checked, in ChainState::apply_linear_block) when that phase actually
    /// has an allowlist root; a Public phase (root = None) needs neither.
    /// Deliberately NOT bound into the signing message - it's derived data
    /// the signer doesn't need to commit to, so a mint can be resubmitted
    /// with a refreshed proof (e.g. against a re-gossiped allowlist) without
    /// invalidating the signature.
    pub allowlist_proof: Option<Vec<[u8; 32]>>,
    pub allowlist_leaf_index: Option<u32>,
    /// If Some, this mint only becomes valid once a TxKernel with this exact
    /// `excess` commitment exists on this chain (see ChainState::kernel_excesses)
    /// - the same trustless atomic-swap primitive TransferAssetOp already
    /// uses, applied to minting. None behaves exactly like an ordinary
    /// (non-collection or free) mint.
    pub required_kernel_excess: Option<Commitment>,
    /// Signed by `owner_pubkey`'s own secret key - unlike TransferAssetOp
    /// (signed by the asset's *current* owner, i.e. the seller), a mint's
    /// `owner_pubkey` is the NEW owner (the buyer/minter), since nobody else
    /// could plausibly authorize claiming a not-yet-existing asset_id.
    pub signature: Signature,
    /// Required (checked in ChainState::apply_linear_block) whenever
    /// collection_id is Some - the collection's creator_pubkey's signature
    /// over collection_approval_signing_message, explicitly approving THIS
    /// exact (asset_id, collection_id, phase_index, required_kernel_excess,
    /// owner_pubkey) combination.
    ///
    /// This is load-bearing, not decorative: `signature` above is signed by
    /// the BUYER (the new owner), who has no incentive to only reference a
    /// kernel that genuinely paid the creator - a buyer could otherwise set
    /// required_kernel_excess to any unrelated kernel already sitting in the
    /// chain's public kernel_excesses set (e.g. a stranger's old
    /// transaction) and mint for free, since apply_linear_block only checks
    /// kernel *existence*, never who it paid or how much. Requiring the
    /// creator's own separate signature over this specific combination
    /// means the creator - the party who actually cares about being paid -
    /// is the one who verifies (off-chain, in their own wallet) that the
    /// referenced kernel really is a real payment of the phase's price to
    /// them, before contributing this signature. None for a non-collection
    /// mint (no creator to approve).
    pub creator_signature: Option<Signature>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetError {
    TooShort,
    TooLong,
    InvalidCharacters,
    AlreadyMinted,
    InvalidSignature,
    InvalidFeePayment,
    MetadataTooLong,
    /// collection_id and phase_index must both be Some or both be None -
    /// never just one (checked in validate_standalone; chain-state-dependent
    /// collection/phase/allowlist/quota checks live in apply_linear_block).
    CollectionFieldsMismatched,
    /// A collection-tagged mint is missing the creator's approval signature,
    /// or it doesn't verify - see MintAssetOp::creator_signature's doc
    /// comment for why this is required.
    MissingOrInvalidCreatorApproval,
    /// allowlist_proof longer than any real Merkle proof could need - see
    /// MAX_ALLOWLIST_PROOF_LENGTH.
    AllowlistProofTooLong,
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
    /// asset_id and metadata together with the collection-drop fields
    /// (collection_id, phase_index, required_kernel_excess), so a mint
    /// signature can't later be replayed to claim a different asset_id/
    /// metadata, be redirected to a different collection/phase, or have its
    /// payment condition stripped or swapped - same domain-separation
    /// discipline TransferAssetOp::signing_message uses for
    /// required_kernel_excess (a tag byte per Option before its payload, so
    /// None and every distinct Some(_) produce distinct byte sequences).
    /// allowlist_proof/allowlist_leaf_index are deliberately NOT bound here -
    /// see their doc comments on MintAssetOp.
    pub fn signing_message(asset_id: &str, metadata: &[u8], collection_id: &Option<String>, phase_index: &Option<u32>, required_kernel_excess: &Option<Commitment>) -> Vec<u8> {
        let mut msg = asset_id.as_bytes().to_vec();
        msg.extend_from_slice(metadata);
        match collection_id {
            Some(id) => {
                msg.push(1u8);
                msg.extend_from_slice(id.as_bytes());
            }
            None => msg.push(0u8),
        }
        match phase_index {
            Some(i) => {
                msg.push(1u8);
                msg.extend_from_slice(&i.to_le_bytes());
            }
            None => msg.push(0u8),
        }
        match required_kernel_excess {
            Some(c) => {
                msg.push(1u8);
                msg.extend_from_slice(c.as_point().compress().as_bytes());
            }
            None => msg.push(0u8),
        }
        msg
    }

    pub fn sign(asset_id: &str, metadata: &[u8], collection_id: &Option<String>, phase_index: &Option<u32>, required_kernel_excess: &Option<Commitment>, owner_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(asset_id, metadata, collection_id, phase_index, required_kernel_excess), owner_secret)
    }

    /// Message the collection's creator signs to approve one specific mint -
    /// see MintAssetOp::creator_signature's doc comment for why this exists.
    /// Deliberately a distinct, narrower message from signing_message above
    /// (doesn't include metadata - the creator is approving a payment/phase/
    /// owner combination, not vouching for arbitrary buyer-supplied
    /// metadata text).
    pub fn collection_approval_signing_message(asset_id: &str, collection_id: &str, phase_index: u32, required_kernel_excess: &Commitment, owner_pubkey: &Commitment) -> Vec<u8> {
        let mut msg = b"HazeCollectionMintApproval:".to_vec();
        msg.extend_from_slice(asset_id.as_bytes());
        msg.extend_from_slice(collection_id.as_bytes());
        msg.extend_from_slice(&phase_index.to_le_bytes());
        msg.extend_from_slice(required_kernel_excess.as_point().compress().as_bytes());
        msg.extend_from_slice(owner_pubkey.as_point().compress().as_bytes());
        msg
    }

    pub fn sign_collection_approval(asset_id: &str, collection_id: &str, phase_index: u32, required_kernel_excess: &Commitment, owner_pubkey: &Commitment, creator_secret: &Scalar) -> Signature {
        Signature::sign(&Self::collection_approval_signing_message(asset_id, collection_id, phase_index, required_kernel_excess, owner_pubkey), creator_secret)
    }

    /// Validates this op in isolation (asset_id rules, metadata length,
    /// collection/phase field pairing, signature, fee-payment's own internal
    /// balance/proof correctness). Does NOT check asset_id uniqueness, that
    /// fee_payment's inputs are real unspent UTXOs, collection/phase
    /// existence, allowlist membership, per-wallet quotas, or the
    /// required_kernel_excess condition - all of those require chain state
    /// and are checked separately by the caller (ChainState::apply_linear_block).
    pub fn validate_standalone(&self) -> Result<(), AssetError> {
        validate_asset_id(&self.asset_id)?;

        if self.metadata.len() > MAX_METADATA_BYTES {
            return Err(AssetError::MetadataTooLong);
        }

        if self.collection_id.is_some() != self.phase_index.is_some() {
            return Err(AssetError::CollectionFieldsMismatched);
        }

        if let Some(proof) = &self.allowlist_proof {
            if proof.len() > MAX_ALLOWLIST_PROOF_LENGTH {
                return Err(AssetError::AllowlistProofTooLong);
            }
        }

        let msg = Self::signing_message(&self.asset_id, &self.metadata, &self.collection_id, &self.phase_index, &self.required_kernel_excess);
        if !self.signature.verify(&msg, &self.owner_pubkey) {
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
    /// If Some, this transfer only becomes valid once a TxKernel with this
    /// exact `excess` commitment exists on this chain (see
    /// ChainState::kernel_excesses) - the trustless atomic-swap primitive.
    /// A seller can sign this unconditionally-looking transfer BEFORE a
    /// buyer's payment is on-chain, because it is cryptographically inert
    /// (fails block-apply) until that payment's kernel actually lands.
    /// Bound into the signed message below so a relayer can't strip the
    /// condition off and get an unconditional transfer through. `None`
    /// behaves exactly like an ordinary unconditional transfer.
    pub required_kernel_excess: Option<Commitment>,
    /// If the asset's collection (see AssetRecord.collection_id) has a
    /// nonzero royalty_bps, this must be Some and a TxKernel with this
    /// exact excess must exist on-chain before the transfer applies - the
    /// same trustless-payment primitive as required_kernel_excess, just a
    /// second independent condition, so a resale can't complete without
    /// BOTH the seller's proceeds AND the original creator's cut actually
    /// landing (see ChainState::apply_linear_block's transfer_asset_ops
    /// loop). Ignored (need not be Some) when the asset has no collection
    /// or that collection's royalty_bps is 0.
    pub required_royalty_kernel_excess: Option<Commitment>,
    /// Signed by the *current* owner's secret key - verified against
    /// whatever AssetRecord.owner_pubkey currently is, not anything in this
    /// struct, so a stale/forged transfer can't just supply its own key.
    pub signature: Signature,
}

impl TransferAssetOp {
    /// Domain-separated from MintAssetOp::signing_message (distinct prefix)
    /// so a mint signature can never be replayed as a transfer signature or
    /// vice versa, and binds in the new owner pubkey (so a transfer can't be
    /// redirected to a different destination after the fact) and both
    /// payment conditions (so neither can be stripped or retargeted after
    /// signing). `None` and every distinct `Some(_)` produce distinct byte
    /// sequences for each field independently.
    pub fn signing_message(asset_id: &str, new_owner_pubkey: &Commitment, required_kernel_excess: &Option<Commitment>, required_royalty_kernel_excess: &Option<Commitment>) -> Vec<u8> {
        let mut msg = b"HazeAssetTransfer:".to_vec();
        msg.extend_from_slice(asset_id.as_bytes());
        msg.extend_from_slice(new_owner_pubkey.as_point().compress().as_bytes());
        match required_kernel_excess {
            Some(c) => {
                msg.push(1u8);
                msg.extend_from_slice(c.as_point().compress().as_bytes());
            }
            None => msg.push(0u8),
        }
        match required_royalty_kernel_excess {
            Some(c) => {
                msg.push(1u8);
                msg.extend_from_slice(c.as_point().compress().as_bytes());
            }
            None => msg.push(0u8),
        }
        msg
    }

    pub fn sign(asset_id: &str, new_owner_pubkey: &Commitment, required_kernel_excess: &Option<Commitment>, required_royalty_kernel_excess: &Option<Commitment>, current_owner_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(asset_id, new_owner_pubkey, required_kernel_excess, required_royalty_kernel_excess), current_owner_secret)
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
            metadata: vec![1u8; 4],
            minted_at_block: 1,
            collection_id: None,
        };
        let record_b = AssetRecord {
            asset_id: "bob-punk".to_string(),
            owner_pubkey: Commitment::new(0, blinding_b),
            metadata: vec![2u8; 4],
            minted_at_block: 2,
            collection_id: None,
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
        let metadata_hash = b"some metadata".to_vec();

        let sig = MintAssetOp::sign("cryptopunk", &metadata_hash, &None, &None, &None, &secret);
        assert!(sig.verify(&MintAssetOp::signing_message("cryptopunk", &metadata_hash, &None, &None, &None), &owner_pubkey));
        assert!(!sig.verify(&MintAssetOp::signing_message("someoneelse", &metadata_hash, &None, &None, &None), &owner_pubkey));
        assert!(!sig.verify(&MintAssetOp::signing_message("cryptopunk", b"different metadata", &None, &None, &None), &owner_pubkey));

        let other_secret = Scalar::from(8u64);
        let other_pubkey = Commitment(other_secret * gens.B_blinding);
        assert!(!sig.verify(&MintAssetOp::signing_message("cryptopunk", &metadata_hash, &None, &None, &None), &other_pubkey));
    }

    #[test]
    fn transfer_signing_message_is_domain_separated_from_mint() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata_hash = b"some metadata".to_vec();

        let mint_sig = MintAssetOp::sign("cryptopunk", &metadata_hash, &None, &None, &None, &secret);
        // A mint signature must never verify as a valid transfer signature
        // for the same asset_id, even targeting the same pubkey.
        assert!(!mint_sig.verify(&TransferAssetOp::signing_message("cryptopunk", &owner_pubkey, &None, &None), &owner_pubkey));
    }

    /// Mirrors transfer_signing_message_binds_required_kernel_excess: the
    /// same tamper-resistance property now applies to a collection-drop
    /// mint's collection_id, phase_index, and required_kernel_excess.
    #[test]
    fn mint_signing_message_binds_collection_phase_and_required_kernel_excess() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata = b"meta".to_vec();
        let kernel_excess_a = Commitment::new(0, Scalar::from(100u64));
        let kernel_excess_b = Commitment::new(0, Scalar::from(200u64));

        let collection_id = Some("cryptopunks".to_string());
        let phase_index = Some(1u32);
        let required = Some(kernel_excess_a);

        let sig = MintAssetOp::sign("punk-1", &metadata, &collection_id, &phase_index, &required, &secret);

        assert!(sig.verify(&MintAssetOp::signing_message("punk-1", &metadata, &collection_id, &phase_index, &required), &owner_pubkey));

        // Different collection_id.
        assert!(!sig.verify(&MintAssetOp::signing_message("punk-1", &metadata, &Some("other".to_string()), &phase_index, &required), &owner_pubkey));
        // No collection_id at all.
        assert!(!sig.verify(&MintAssetOp::signing_message("punk-1", &metadata, &None, &phase_index, &required), &owner_pubkey));
        // Different phase_index.
        assert!(!sig.verify(&MintAssetOp::signing_message("punk-1", &metadata, &collection_id, &Some(2u32), &required), &owner_pubkey));
        // Different required_kernel_excess.
        assert!(!sig.verify(&MintAssetOp::signing_message("punk-1", &metadata, &collection_id, &phase_index, &Some(kernel_excess_b)), &owner_pubkey));
        // Stripped required_kernel_excess.
        assert!(!sig.verify(&MintAssetOp::signing_message("punk-1", &metadata, &collection_id, &phase_index, &None), &owner_pubkey));
    }

    #[test]
    fn mint_signing_message_none_vs_some_are_distinct_for_every_new_field() {
        let metadata = b"meta".to_vec();
        let msg_base = MintAssetOp::signing_message("punk-1", &metadata, &None, &None, &None);

        let msg_with_collection = MintAssetOp::signing_message("punk-1", &metadata, &Some("c".to_string()), &None, &None);
        let msg_with_phase = MintAssetOp::signing_message("punk-1", &metadata, &None, &Some(0u32), &None);
        let msg_with_excess = MintAssetOp::signing_message("punk-1", &metadata, &None, &None, &Some(Commitment::new(0, Scalar::from(0u64))));

        assert_ne!(msg_base, msg_with_collection);
        assert_ne!(msg_base, msg_with_phase);
        assert_ne!(msg_base, msg_with_excess);
    }

    #[test]
    fn validate_standalone_rejects_mismatched_collection_fields() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata = b"meta".to_vec();

        // collection_id Some, phase_index None - must be rejected before
        // even reaching the signature check.
        let collection_id = Some("cryptopunks".to_string());
        let signature = MintAssetOp::sign("punk-1", &metadata, &collection_id, &None, &None, &secret);
        let op = MintAssetOp {
            asset_id: "punk-1".to_string(),
            owner_pubkey,
            metadata,
            fee_payment: crate::core::transaction::Transaction { inputs: vec![], outputs: vec![], kernels: vec![] },
            collection_id,
            phase_index: None,
            allowlist_proof: None,
            allowlist_leaf_index: None,
            required_kernel_excess: None,
            signature,
            creator_signature: None,
        };
        assert_eq!(op.validate_standalone(), Err(AssetError::CollectionFieldsMismatched));
    }

    #[test]
    fn validate_standalone_rejects_an_oversized_allowlist_proof() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata = b"meta".to_vec();
        let collection_id = Some("cryptopunks".to_string());
        let phase_index = Some(0u32);
        let signature = MintAssetOp::sign("punk-1", &metadata, &collection_id, &phase_index, &None, &secret);
        let op = MintAssetOp {
            asset_id: "punk-1".to_string(),
            owner_pubkey,
            metadata,
            fee_payment: crate::core::transaction::Transaction { inputs: vec![], outputs: vec![], kernels: vec![] },
            collection_id,
            phase_index,
            allowlist_proof: Some(vec![[0u8; 32]; MAX_ALLOWLIST_PROOF_LENGTH + 1]),
            allowlist_leaf_index: Some(0),
            required_kernel_excess: None,
            signature,
            creator_signature: None,
        };
        assert_eq!(op.validate_standalone(), Err(AssetError::AllowlistProofTooLong));
    }

    /// The single most important test in this module: the entire
    /// tamper-resistance property of the trustless-swap design depends on
    /// required_kernel_excess being bound into the signed message. A
    /// signature over one condition (or no condition at all) must never
    /// verify against a message claiming a different condition.
    #[test]
    fn transfer_signing_message_binds_required_kernel_excess() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let new_owner = Commitment::new(0, Scalar::from(42u64));
        let kernel_excess_a = Commitment::new(0, Scalar::from(100u64));
        let kernel_excess_b = Commitment::new(0, Scalar::from(200u64));

        let sig_conditional_a = TransferAssetOp::sign("cryptopunk", &new_owner, &Some(kernel_excess_a), &None, &secret);

        // Valid against its own exact condition.
        assert!(sig_conditional_a.verify(
            &TransferAssetOp::signing_message("cryptopunk", &new_owner, &Some(kernel_excess_a), &None),
            &owner_pubkey,
        ));
        // A relayer must not be able to swap in a different required kernel...
        assert!(!sig_conditional_a.verify(
            &TransferAssetOp::signing_message("cryptopunk", &new_owner, &Some(kernel_excess_b), &None),
            &owner_pubkey,
        ));
        // ...or strip the condition entirely to make it unconditional.
        assert!(!sig_conditional_a.verify(
            &TransferAssetOp::signing_message("cryptopunk", &new_owner, &None, &None),
            &owner_pubkey,
        ));

        // And the reverse: an unconditional signature must not verify as
        // satisfying any particular condition.
        let sig_unconditional = TransferAssetOp::sign("cryptopunk", &new_owner, &None, &None, &secret);
        assert!(!sig_unconditional.verify(
            &TransferAssetOp::signing_message("cryptopunk", &new_owner, &Some(kernel_excess_a), &None),
            &owner_pubkey,
        ));
    }

    #[test]
    fn transfer_signing_message_none_vs_some_are_distinct() {
        let new_owner = Commitment::new(0, Scalar::from(42u64));
        let msg_none = TransferAssetOp::signing_message("cryptopunk", &new_owner, &None, &None);
        let msg_some = TransferAssetOp::signing_message("cryptopunk", &new_owner, &Some(Commitment::new(0, Scalar::from(0u64))), &None);
        assert_ne!(msg_none, msg_some, "None and Some(_) must never produce identical signing messages, even for a zero-ish commitment");
    }

    /// Mirrors transfer_signing_message_binds_required_kernel_excess: the
    /// same tamper-resistance property for the independent royalty
    /// condition.
    #[test]
    fn transfer_signing_message_binds_required_royalty_kernel_excess() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let new_owner = Commitment::new(0, Scalar::from(42u64));
        let royalty_a = Commitment::new(0, Scalar::from(300u64));
        let royalty_b = Commitment::new(0, Scalar::from(400u64));

        let sig = TransferAssetOp::sign("cryptopunk", &new_owner, &None, &Some(royalty_a), &secret);

        assert!(sig.verify(&TransferAssetOp::signing_message("cryptopunk", &new_owner, &None, &Some(royalty_a)), &owner_pubkey));
        assert!(!sig.verify(&TransferAssetOp::signing_message("cryptopunk", &new_owner, &None, &Some(royalty_b)), &owner_pubkey));
        assert!(!sig.verify(&TransferAssetOp::signing_message("cryptopunk", &new_owner, &None, &None), &owner_pubkey));
    }

    #[test]
    fn accepts_metadata_at_max_length() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata = vec![0u8; MAX_METADATA_BYTES];
        let signature = MintAssetOp::sign("cryptopunk", &metadata, &None, &None, &None, &secret);
        let op = MintAssetOp {
            asset_id: "cryptopunk".to_string(),
            owner_pubkey,
            metadata,
            fee_payment: crate::core::transaction::Transaction { inputs: vec![], outputs: vec![], kernels: vec![] },
            collection_id: None,
            phase_index: None,
            allowlist_proof: None,
            allowlist_leaf_index: None,
            required_kernel_excess: None,
            signature,
            creator_signature: None,
        };
        // Only the metadata-length check should pass here; fee_payment is
        // deliberately empty/invalid so this asserts on the specific error,
        // not overall success.
        assert_ne!(op.validate_standalone(), Err(AssetError::MetadataTooLong));
    }

    #[test]
    fn rejects_metadata_over_max_length() {
        let secret = Scalar::from(7u64);
        let gens = bulletproofs::PedersenGens::default();
        let owner_pubkey = Commitment(secret * gens.B_blinding);
        let metadata = vec![0u8; MAX_METADATA_BYTES + 1];
        let signature = MintAssetOp::sign("cryptopunk", &metadata, &None, &None, &None, &secret);
        let op = MintAssetOp {
            asset_id: "cryptopunk".to_string(),
            owner_pubkey,
            metadata,
            fee_payment: crate::core::transaction::Transaction { inputs: vec![], outputs: vec![], kernels: vec![] },
            collection_id: None,
            phase_index: None,
            allowlist_proof: None,
            allowlist_leaf_index: None,
            required_kernel_excess: None,
            signature,
            creator_signature: None,
        };
        assert_eq!(op.validate_standalone(), Err(AssetError::MetadataTooLong));
    }
}
