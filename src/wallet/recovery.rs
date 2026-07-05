//! Restore-from-phrase: reconstructing a wallet's balance purely from its
//! BIP39-derived keystore and the chain's public scan data, with no locally
//! persisted WalletStore needed at all.
//!
//! This is the actual mechanism that makes "restore from phrase" real
//! rather than cosmetic: Keystore::derive_blinding is deterministic
//! (seed + index), so every blinding factor a wallet ever used is
//! re-derivable from the seed alone, and every output it ever creates
//! (self-pay dest/change - wallet::planner, two-party change/receive -
//! wallet::slate, block rewards - core::proposer/wallet::note's
//! coinbase_blinding) carries a note encrypted under a key derived the same
//! way (Keystore::note_key) - so recovery doesn't depend on any local index
//! bookkeeping surviving, only on the phrase itself plus what's already
//! public on-chain (see api::explorer::handle_scan_outputs).
use std::collections::HashSet;

use crate::crypto::pedersen::Commitment;
use super::keystore::Keystore;
use super::store::{WalletStore, OutputStatus};
use super::note;

pub struct ScanEntry {
    pub commitment_hex: String,
    pub note_hex: String,
}

pub struct RecoveryResult {
    pub store: WalletStore,
    pub recovered_count: u32,
    pub recovered_balance: u64,
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len()).step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// Tries every scanned output's note against this keystore's own note_key.
/// Only entries that (a) decrypt successfully AND (b) reproduce their exact
/// on-chain commitment when re-derived from the recovered (index, value) are
/// trusted - decryption alone is already strong proof of ownership
/// (ChaCha20-Poly1305's auth tag), the commitment re-check is a final sanity
/// check against a corrupted/truncated note that happened to still pass the
/// AEAD tag. Only entries still present in `utxo_set` (i.e. unspent) count
/// toward the recovered balance and get added back to the returned store,
/// as Confirmed. Also bumps `keystore`'s next_index past every recovered
/// index (spent or not), so newly allocated indices after a restore can't
/// collide with ones this wallet already used.
pub fn recover_from_chain(keystore: &mut Keystore, entries: &[ScanEntry], utxo_set: &HashSet<String>) -> RecoveryResult {
    let note_key = keystore.note_key();
    let mut store = WalletStore::default();
    let mut max_index_seen: Option<u32> = None;
    let mut recovered_balance: u64 = 0;
    let mut recovered_count: u32 = 0;

    for entry in entries {
        let Some(note_bytes) = hex_decode(&entry.note_hex) else { continue };
        let Some((index, value)) = note::open(&note_key, &note_bytes) else { continue };

        let expected_commitment = Commitment::new(value, keystore.derive_blinding(index));
        if expected_commitment.to_hex() != entry.commitment_hex {
            continue;
        }

        max_index_seen = Some(max_index_seen.map_or(index, |m| m.max(index)));

        if utxo_set.contains(&entry.commitment_hex) {
            store.add_output(index, value, expected_commitment, OutputStatus::Confirmed);
            recovered_balance += value;
            recovered_count += 1;
        }
    }

    if let Some(max_index) = max_index_seen {
        keystore.ensure_next_index_at_least(max_index + 1);
    }

    RecoveryResult { store, recovered_count, recovered_balance }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::transaction::Output;
    use crate::crypto::range_proof::RangeProof;
    use crate::wallet::planner;

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn make_output(keystore: &Keystore, index: u32, value: u64) -> Output {
        let blinding = keystore.derive_blinding(index);
        let commitment = Commitment::new(value, blinding);
        let proof = RangeProof::prove(value, &blinding);
        let n = note::seal(&keystore.note_key(), index, value);
        Output { commitment, proof, note: n }
    }

    fn scan_entry(output: &Output) -> ScanEntry {
        ScanEntry { commitment_hex: output.commitment.to_hex(), note_hex: to_hex(&output.note) }
    }

    /// End-to-end fund-safety proof: a wallet with real activity (a funding
    /// output, then a self-send producing a fresh dest + change output) must
    /// be fully reconstructible - balance and spendability both - from
    /// nothing but its BIP39 phrase plus what the chain already publishes.
    #[test]
    fn full_restore_from_phrase_reproduces_balance_and_can_still_spend() {
        let (mut original_keystore, phrase) = Keystore::generate_with_mnemonic();

        // Fund the wallet with one output, then self-send part of it so
        // there's real dest+change activity to recover, not just one output.
        let funding_index = original_keystore.allocate_index();
        let funding_output = make_output(&original_keystore, funding_index, 1_000);

        let mut store = WalletStore::default();
        store.add_output(funding_index, 1_000, funding_output.commitment, OutputStatus::Confirmed);

        let plan = planner::plan_send(&mut original_keystore, &store, 200, 5).expect("plan_send should succeed");
        assert!(plan.transaction.validate());

        // Simulate the send confirming on-chain.
        store.mark_spent(&funding_output.commitment);
        let (dest_index, dest_commitment, dest_value) = plan.dest;
        store.add_output(dest_index, dest_value, dest_commitment, OutputStatus::Confirmed);
        let mut expected_utxos: HashSet<String> = HashSet::new();
        expected_utxos.insert(dest_commitment.to_hex());
        if let Some((change_index, change_commitment, change_value)) = plan.change {
            store.add_output(change_index, change_value, change_commitment, OutputStatus::Confirmed);
            expected_utxos.insert(change_commitment.to_hex());
        }

        let original_balance = store.balance();
        // The initial fee guess of 5 may have been auto-corrected upward by
        // plan_send to match this (1-input/2-output) transaction's real
        // size - amount + change both still ours, only whatever fee was
        // actually paid left the wallet.
        let paid_fee = plan.transaction.kernels[0].fee;
        assert_eq!(original_balance, 1_000 - paid_fee);

        // Everything the chain would publish for this wallet's activity:
        // the funding output (now spent) plus dest/change (still unspent).
        let mut scan_entries = vec![scan_entry(&funding_output)];
        let dest_output = make_output(&original_keystore, dest_index, dest_value);
        scan_entries.push(scan_entry(&dest_output));
        if let Some((change_index, _, change_value)) = plan.change {
            let change_output = make_output(&original_keystore, change_index, change_value);
            scan_entries.push(scan_entry(&change_output));
        }

        // A completely fresh wallet instance, constructed from only the phrase.
        let mut restored_keystore = Keystore::from_mnemonic(&phrase).expect("phrase must parse");
        let result = recover_from_chain(&mut restored_keystore, &scan_entries, &expected_utxos);

        assert_eq!(result.recovered_balance, original_balance, "restored balance must match exactly");
        assert_eq!(result.store.balance(), original_balance);

        // And it must still be able to spend - proving the recovered
        // blinding factors are the real ones, not just bookkeeping that
        // happens to add up.
        let spend_plan = planner::plan_send(&mut restored_keystore, &result.store, 100, 5)
            .expect("restored wallet must be able to plan a real spend");
        assert!(spend_plan.transaction.validate(), "restored wallet's spend must be a genuinely valid transaction");
    }

    #[test]
    fn recovery_ignores_a_note_sealed_under_a_different_wallets_key() {
        let (keystore, phrase) = Keystore::generate_with_mnemonic();
        let (other_keystore, _) = Keystore::generate_with_mnemonic();

        let foreign_output = make_output(&other_keystore, 0, 500);
        let entries = vec![scan_entry(&foreign_output)];
        let mut utxos = HashSet::new();
        utxos.insert(foreign_output.commitment.to_hex());

        let mut restored = Keystore::from_mnemonic(&phrase).expect("phrase must parse");
        let _ = keystore; // keep the original alive for clarity, unused otherwise
        let result = recover_from_chain(&mut restored, &entries, &utxos);

        assert_eq!(result.recovered_count, 0);
        assert_eq!(result.recovered_balance, 0);
    }
}
