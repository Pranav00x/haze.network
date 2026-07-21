//! Off-chain (non-consensus) distribution of a collection phase's full
//! plaintext allowlist - mirrors core::marketplace's Listing/MarketplaceState
//! pattern exactly. Only a phase's Merkle root is consensus (see
//! core::collections::MintPhase); the underlying pubkey list is gossiped
//! like a mempool entry so any wallet/marketplace client can fetch it and
//! compute its own inclusion proof client-side (see core::merkle), without
//! needing a dedicated proof-storage server.

use std::collections::HashMap;
use std::sync::Mutex;
use crate::sync::LockExt;
use serde::{Serialize, Deserialize};
use curve25519_dalek_ng::scalar::Scalar;

use haze_crypto::pedersen::Commitment;
use haze_crypto::schnorr::Signature;
use super::collections::CollectionRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AllowlistEntry {
    pub collection_id: String,
    pub phase_index: u32,
    pub creator_pubkey: Commitment,
    pub pubkeys: Vec<Commitment>,
    /// Unix timestamp, informational only - same role as Listing::listed_at
    /// (no expiry/ordering semantics beyond last-write-wins per key).
    pub published_at: u64,
    pub signature: Signature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowlistError {
    InvalidSignature,
    /// The signer isn't this collection's registered creator_pubkey - either
    /// they never were, or this is a stale/fraudulent publish for someone
    /// else's collection.
    NotCollectionCreator,
    /// pubkeys longer than any real allowlist plausibly needs - see
    /// MAX_ALLOWLIST_ENTRIES. Checked before signature verification
    /// specifically so an oversized entry is rejected cheaply, without
    /// spending a real signature-verify (or the hash-the-whole-array work
    /// signing_message does) on it - self-signing an entry costs an
    /// attacker nothing, they don't need to actually own any collection to
    /// submit one (see validate_against_registry, a separate, later check).
    TooManyEntries,
}

/// A real allowlist - even a large, popular collection's early-access list -
/// tops out at a few tens of thousands of addresses. 200,000 is generous
/// headroom while still bounding the cost of gossiping/validating one of
/// these, which (unlike a mint) costs its submitter nothing at all.
pub const MAX_ALLOWLIST_ENTRIES: usize = 200_000;

impl AllowlistEntry {
    /// Binds collection_id, phase_index, the full pubkey list, and
    /// published_at, so a publish can't be replayed for a different phase or
    /// with a different (e.g. trimmed/injected) pubkey list than what the
    /// creator actually signed.
    pub fn signing_message(collection_id: &str, phase_index: u32, pubkeys: &[Commitment], published_at: u64) -> Vec<u8> {
        let mut msg = b"HazeAllowlistPublish:".to_vec();
        msg.extend_from_slice(collection_id.as_bytes());
        msg.extend_from_slice(&phase_index.to_le_bytes());
        for pk in pubkeys {
            msg.extend_from_slice(pk.as_point().compress().as_bytes());
        }
        msg.extend_from_slice(&published_at.to_le_bytes());
        msg
    }

    pub fn sign(collection_id: &str, phase_index: u32, pubkeys: &[Commitment], published_at: u64, creator_secret: &Scalar) -> Signature {
        Signature::sign(&Self::signing_message(collection_id, phase_index, pubkeys, published_at), creator_secret)
    }

    /// Proves the signer controls creator_pubkey at publish time - does NOT
    /// prove creator_pubkey is actually this collection's registered
    /// creator (see validate_against_registry, checked separately since it
    /// needs chain state).
    pub fn validate_standalone(&self) -> Result<(), AllowlistError> {
        if self.pubkeys.len() > MAX_ALLOWLIST_ENTRIES {
            return Err(AllowlistError::TooManyEntries);
        }
        let msg = Self::signing_message(&self.collection_id, self.phase_index, &self.pubkeys, self.published_at);
        if !self.signature.verify(&msg, &self.creator_pubkey) {
            return Err(AllowlistError::InvalidSignature);
        }
        Ok(())
    }

    pub fn validate_against_registry(&self, collection_registry: &HashMap<String, CollectionRecord>) -> Result<(), AllowlistError> {
        match collection_registry.get(&self.collection_id) {
            Some(record) if record.creator_pubkey == self.creator_pubkey => Ok(()),
            _ => Err(AllowlistError::NotCollectionCreator),
        }
    }
}

/// In-memory, best-effort allowlist registry - one entry per
/// (collection_id, phase_index) (last-write-wins), gossiped via P2P, never
/// persisted or committed into consensus state.
#[derive(Default)]
pub struct AllowlistState {
    entries: Mutex<HashMap<(String, u32), AllowlistEntry>>,
}

impl AllowlistState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&self, entry: AllowlistEntry) {
        let mut entries = self.entries.lock_recover();
        entries.insert((entry.collection_id.clone(), entry.phase_index), entry);
    }

    pub fn get(&self, collection_id: &str, phase_index: u32) -> Option<AllowlistEntry> {
        self.entries.lock_recover().get(&(collection_id.to_string(), phase_index)).cloned()
    }

    pub fn list_all(&self) -> Vec<AllowlistEntry> {
        self.entries.lock_recover().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bulletproofs::PedersenGens;

    fn make_signed_entry(collection_id: &str, phase_index: u32, creator_secret: &Scalar, pubkeys: Vec<Commitment>) -> (AllowlistEntry, Commitment) {
        let gens = PedersenGens::default();
        let creator_pubkey = Commitment(creator_secret * gens.B_blinding);
        let published_at = 1_700_000_000;
        let signature = AllowlistEntry::sign(collection_id, phase_index, &pubkeys, published_at, creator_secret);
        (AllowlistEntry { collection_id: collection_id.to_string(), phase_index, creator_pubkey, pubkeys, published_at, signature }, creator_pubkey)
    }

    #[test]
    fn validate_standalone_accepts_a_correctly_signed_entry() {
        let secret = Scalar::from(11u64);
        let member = Commitment::new(0, Scalar::from(1u64));
        let (entry, _) = make_signed_entry("cryptopunks", 0, &secret, vec![member]);
        assert!(entry.validate_standalone().is_ok());
    }

    #[test]
    fn validate_standalone_rejects_an_oversized_allowlist() {
        let secret = Scalar::from(11u64);
        let member = Commitment::new(0, Scalar::from(1u64));
        let pubkeys = vec![member; MAX_ALLOWLIST_ENTRIES + 1];
        let (entry, _) = make_signed_entry("cryptopunks", 0, &secret, pubkeys);
        assert_eq!(entry.validate_standalone(), Err(AllowlistError::TooManyEntries));
    }

    #[test]
    fn validate_standalone_rejects_a_tampered_pubkey_list() {
        let secret = Scalar::from(11u64);
        let member = Commitment::new(0, Scalar::from(1u64));
        let (mut entry, _) = make_signed_entry("cryptopunks", 0, &secret, vec![member]);
        entry.pubkeys.push(Commitment::new(0, Scalar::from(999u64))); // injected after signing
        assert_eq!(entry.validate_standalone(), Err(AllowlistError::InvalidSignature));
    }

    #[test]
    fn validate_against_registry_rejects_a_non_creator() {
        let secret = Scalar::from(11u64);
        let member = Commitment::new(0, Scalar::from(1u64));
        let (entry, creator_pubkey) = make_signed_entry("cryptopunks", 0, &secret, vec![member]);

        let mut registry = HashMap::new();
        registry.insert("cryptopunks".to_string(), CollectionRecord {
            collection_id: "cryptopunks".to_string(),
            creator_pubkey,
            name: "CryptoPunks".to_string(),
            symbol: "PUNK".to_string(),
            metadata: vec![],
            phases: vec![],
            launched_at_block: 1,
            royalty_bps: 0,
        });
        assert!(entry.validate_against_registry(&registry).is_ok());

        registry.get_mut("cryptopunks").unwrap().creator_pubkey = Commitment::new(0, Scalar::from(999u64));
        assert_eq!(entry.validate_against_registry(&registry), Err(AllowlistError::NotCollectionCreator));
    }

    #[test]
    fn publish_is_last_write_wins_per_collection_and_phase() {
        let state = AllowlistState::new();
        let secret = Scalar::from(11u64);
        let member_a = Commitment::new(0, Scalar::from(1u64));
        let member_b = Commitment::new(0, Scalar::from(2u64));
        let (entry_a, _) = make_signed_entry("cryptopunks", 0, &secret, vec![member_a]);
        let (entry_b, _) = make_signed_entry("cryptopunks", 0, &secret, vec![member_a, member_b]);

        state.publish(entry_a);
        state.publish(entry_b);

        assert_eq!(state.list_all().len(), 1);
        assert_eq!(state.get("cryptopunks", 0).unwrap().pubkeys.len(), 2);
    }

    #[test]
    fn different_phase_indices_are_independent_entries() {
        let state = AllowlistState::new();
        let secret = Scalar::from(11u64);
        let member = Commitment::new(0, Scalar::from(1u64));
        let (entry_phase_0, _) = make_signed_entry("cryptopunks", 0, &secret, vec![member]);
        let (entry_phase_1, _) = make_signed_entry("cryptopunks", 1, &secret, vec![member]);

        state.publish(entry_phase_0);
        state.publish(entry_phase_1);

        assert_eq!(state.list_all().len(), 2);
        assert!(state.get("cryptopunks", 0).is_some());
        assert!(state.get("cryptopunks", 1).is_some());
    }
}
