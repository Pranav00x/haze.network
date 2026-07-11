//! Marketplace listings - discovery metadata for the trustless atomic-swap
//! primitive (see core::assets::TransferAssetOp::required_kernel_excess).
//! Deliberately NOT part of consensus: a listing is just an advertisement of
//! intent ("I'll sell asset X for Y"), not a state change requiring global
//! ordering across every node - keeping it out of blocks avoids yet another
//! chain-reset-requiring schema change for something that's inherently
//! best-effort (gossiped like a mempool entry, never persisted forever).
//! Mirrors api::inbox::InboxState's in-memory, no-persistence design.

use std::collections::HashMap;
use std::sync::Mutex;
use serde::{Serialize, Deserialize};
use curve25519_dalek_ng::scalar::Scalar;

use haze_crypto::pedersen::Commitment;
use haze_crypto::schnorr::Signature;
use super::assets::AssetRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Listing {
    pub asset_id: String,
    pub seller_pubkey: Commitment,
    pub price: u64,
    /// Unix timestamp, informational only (display purposes) - not checked
    /// by anything, since a listing has no expiry or ordering semantics
    /// beyond "the most recent one for this asset_id wins" (see
    /// MarketplaceState::add_or_replace).
    pub listed_at: u64,
    pub signature: Signature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingError {
    InvalidSignature,
    /// The signer doesn't currently own this asset per the live registry -
    /// either it was never theirs, or it changed hands since this listing
    /// was created.
    NotCurrentOwner,
}

/// Signing message for a listing cancellation - a distinct signed statement
/// ("I withdraw this listing") from the listing itself, so it needs its own
/// domain-separated message rather than reusing Listing::signing_message.
pub fn cancel_signing_message(asset_id: &str, seller_pubkey: &Commitment) -> Vec<u8> {
    let mut msg = b"HazeMarketplaceCancelListing:".to_vec();
    msg.extend_from_slice(asset_id.as_bytes());
    msg.extend_from_slice(seller_pubkey.as_point().compress().as_bytes());
    msg
}

impl Listing {
    pub fn signing_message(asset_id: &str, seller_pubkey: &Commitment, price: u64, listed_at: u64) -> Vec<u8> {
        let mut msg = b"HazeMarketplaceListing:".to_vec();
        msg.extend_from_slice(asset_id.as_bytes());
        msg.extend_from_slice(seller_pubkey.as_point().compress().as_bytes());
        msg.extend_from_slice(&price.to_le_bytes());
        msg.extend_from_slice(&listed_at.to_le_bytes());
        msg
    }

    pub fn sign(asset_id: &str, seller_pubkey: &Commitment, price: u64, listed_at: u64, seller_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(asset_id, seller_pubkey, price, listed_at), seller_secret)
    }

    /// Proves the signer controls seller_pubkey at listing time - does NOT
    /// prove seller_pubkey currently owns asset_id on-chain, since ownership
    /// can change after a listing is gossiped and this signature alone
    /// can't reflect that (see validate_against_registry, checked
    /// separately at accept time since it needs chain state).
    pub fn validate_standalone(&self) -> Result<(), ListingError> {
        let msg = Self::signing_message(&self.asset_id, &self.seller_pubkey, self.price, self.listed_at);
        if !self.signature.verify(&msg, &self.seller_pubkey) {
            return Err(ListingError::InvalidSignature);
        }
        Ok(())
    }

    pub fn validate_against_registry(&self, asset_registry: &HashMap<String, AssetRecord>) -> Result<(), ListingError> {
        match asset_registry.get(&self.asset_id) {
            Some(record) if record.owner_pubkey == self.seller_pubkey => Ok(()),
            _ => Err(ListingError::NotCurrentOwner),
        }
    }
}

/// In-memory, best-effort marketplace listings registry - one listing per
/// asset_id (last-write-wins), gossiped via P2P like any other pending op,
/// never persisted or committed into consensus state.
#[derive(Default)]
pub struct MarketplaceState {
    listings: Mutex<HashMap<String, Listing>>,
}

impl MarketplaceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_or_replace(&self, listing: Listing) {
        let mut listings = self.listings.lock().unwrap();
        listings.insert(listing.asset_id.clone(), listing);
    }

    /// Only whoever can sign as the listing's own seller_pubkey may cancel
    /// it - returns false (no-op) if there's no listing for this asset_id,
    /// or if requester_pubkey doesn't match. Callers (API/P2P handlers)
    /// are responsible for verifying requester_pubkey's signature over
    /// cancel_signing_message before calling this - this method only
    /// checks the pubkey match, not any signature.
    pub fn cancel(&self, asset_id: &str, requester_pubkey: &Commitment) -> bool {
        let mut listings = self.listings.lock().unwrap();
        if let Some(existing) = listings.get(asset_id) {
            if existing.seller_pubkey == *requester_pubkey {
                listings.remove(asset_id);
                return true;
            }
        }
        false
    }

    pub fn get(&self, asset_id: &str) -> Option<Listing> {
        self.listings.lock().unwrap().get(asset_id).cloned()
    }

    pub fn list_all(&self) -> Vec<Listing> {
        self.listings.lock().unwrap().values().cloned().collect()
    }

    /// Drops any listing for an asset a just-applied block touched (minted
    /// or transferred) - same pattern as
    /// Mempool::clear_stale_transfer_asset_ops, wired into the same
    /// block-apply path. Also sweeps out any listing whose seller no longer
    /// matches the live registry, covering ownership changes this node
    /// didn't itself just apply (e.g. it was offline for a stretch and
    /// resyncs several blocks at once).
    pub fn clear_stale(&self, touched_assets: &[String], asset_registry: &HashMap<String, AssetRecord>) {
        let mut listings = self.listings.lock().unwrap();
        for asset_id in touched_assets {
            listings.remove(asset_id);
        }
        listings.retain(|asset_id, listing| {
            asset_registry.get(asset_id).map(|r| r.owner_pubkey == listing.seller_pubkey).unwrap_or(false)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bulletproofs::PedersenGens;

    fn make_signed_listing(asset_id: &str, seller_secret: &Scalar, price: u64) -> (Listing, Commitment) {
        let gens = PedersenGens::default();
        let seller_pubkey = Commitment(seller_secret * gens.B_blinding);
        let listed_at = 1_700_000_000;
        let signature = Listing::sign(asset_id, &seller_pubkey, price, listed_at, seller_secret);
        (Listing { asset_id: asset_id.to_string(), seller_pubkey, price, listed_at, signature }, seller_pubkey)
    }

    #[test]
    fn validate_standalone_accepts_a_correctly_signed_listing() {
        let secret = Scalar::from(11u64);
        let (listing, _) = make_signed_listing("cryptopunk", &secret, 100);
        assert!(listing.validate_standalone().is_ok());
    }

    #[test]
    fn validate_standalone_rejects_a_tampered_price() {
        let secret = Scalar::from(11u64);
        let (mut listing, _) = make_signed_listing("cryptopunk", &secret, 100);
        listing.price = 1; // tampered after signing
        assert_eq!(listing.validate_standalone(), Err(ListingError::InvalidSignature));
    }

    #[test]
    fn validate_against_registry_rejects_a_non_owner() {
        let secret = Scalar::from(11u64);
        let (listing, seller_pubkey) = make_signed_listing("cryptopunk", &secret, 100);

        let mut registry = HashMap::new();
        registry.insert("cryptopunk".to_string(), AssetRecord {
            asset_id: "cryptopunk".to_string(),
            owner_pubkey: seller_pubkey,
            metadata: vec![],
            minted_at_block: 1,
            collection_id: None,
        });
        assert!(listing.validate_against_registry(&registry).is_ok());

        // A different current owner - the listing is stale/fraudulent.
        registry.get_mut("cryptopunk").unwrap().owner_pubkey = Commitment::new(0, Scalar::from(999u64));
        assert_eq!(listing.validate_against_registry(&registry), Err(ListingError::NotCurrentOwner));
    }

    #[test]
    fn add_or_replace_is_last_write_wins_per_asset() {
        let state = MarketplaceState::new();
        let secret = Scalar::from(11u64);
        let (listing_a, _) = make_signed_listing("cryptopunk", &secret, 100);
        let (listing_b, _) = make_signed_listing("cryptopunk", &secret, 200);

        state.add_or_replace(listing_a);
        state.add_or_replace(listing_b);

        assert_eq!(state.list_all().len(), 1);
        assert_eq!(state.get("cryptopunk").unwrap().price, 200);
    }

    #[test]
    fn cancel_only_succeeds_for_the_listing_seller() {
        let state = MarketplaceState::new();
        let secret = Scalar::from(11u64);
        let (listing, seller_pubkey) = make_signed_listing("cryptopunk", &secret, 100);
        state.add_or_replace(listing);

        let impostor_pubkey = Commitment::new(0, Scalar::from(777u64));
        assert!(!state.cancel("cryptopunk", &impostor_pubkey), "a non-seller must not be able to cancel someone else's listing");
        assert!(state.get("cryptopunk").is_some());

        assert!(state.cancel("cryptopunk", &seller_pubkey));
        assert!(state.get("cryptopunk").is_none());
    }

    #[test]
    fn clear_stale_removes_touched_and_ownership_mismatched_listings() {
        let state = MarketplaceState::new();
        let secret = Scalar::from(11u64);
        let (listing_a, seller_a) = make_signed_listing("touched-asset", &secret, 100);
        let (listing_b, seller_b) = make_signed_listing("stale-owner-asset", &secret, 100);
        let (listing_c, seller_c) = make_signed_listing("still-valid-asset", &secret, 100);
        state.add_or_replace(listing_a);
        state.add_or_replace(listing_b);
        state.add_or_replace(listing_c);

        let mut registry = HashMap::new();
        registry.insert("touched-asset".to_string(), AssetRecord { asset_id: "touched-asset".to_string(), owner_pubkey: seller_a, metadata: vec![], minted_at_block: 1, collection_id: None });
        // stale-owner-asset's registry now shows a DIFFERENT owner than the listing's seller.
        registry.insert("stale-owner-asset".to_string(), AssetRecord { asset_id: "stale-owner-asset".to_string(), owner_pubkey: Commitment::new(0, Scalar::from(42u64)), metadata: vec![], minted_at_block: 1, collection_id: None });
        let _ = seller_b;
        registry.insert("still-valid-asset".to_string(), AssetRecord { asset_id: "still-valid-asset".to_string(), owner_pubkey: seller_c, metadata: vec![], minted_at_block: 1, collection_id: None });

        state.clear_stale(&["touched-asset".to_string()], &registry);

        assert!(state.get("touched-asset").is_none(), "explicitly touched assets must be cleared");
        assert!(state.get("stale-owner-asset").is_none(), "a listing whose seller no longer matches the live registry must be swept");
        assert!(state.get("still-valid-asset").is_some(), "an untouched, still-correctly-owned listing must survive");
    }
}
