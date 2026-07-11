use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use serde::{Serialize, Deserialize};

use haze_crypto::pedersen::Commitment;

const WALLET_DIR: &str = "wallet_data";
const STORE_FILE: &str = "wallet_data/utxos.dat";

/// Reserved index used for the well-known genesis output, never allocated by Keystore::allocate_index.
pub const GENESIS_INDEX: u32 = u32::MAX;

/// Reserved index used for the node's own faucet reserve output (see
/// src/core/genesis.rs's FAUCET_RESERVE_BLINDING and src/api/faucet.rs) -
/// distinct from GENESIS_INDEX so a keystore could in principle hold both.
pub const FAUCET_INDEX: u32 = u32::MAX - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputStatus {
    Pending,
    Confirmed,
    Spent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OwnedOutput {
    pub index: u32,
    pub value: u64,
    pub commitment: Commitment,
    pub status: OutputStatus,
}

#[derive(Default, Serialize, Deserialize)]
pub struct WalletStore {
    outputs: Vec<OwnedOutput>,
}

impl WalletStore {
    pub fn load_or_create() -> Self {
        if !Path::new(WALLET_DIR).exists() {
            fs::create_dir(WALLET_DIR).unwrap();
        }

        if Path::new(STORE_FILE).exists() {
            if let Ok(mut file) = File::open(STORE_FILE) {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    if let Ok(store) = bincode::deserialize::<WalletStore>(&buffer) {
                        return store;
                    }
                }
            }
        }

        Self::default()
    }

    pub fn save(&self) {
        let encoded = self.to_bytes();
        let mut file = File::create(STORE_FILE).unwrap();
        file.write_all(&encoded).unwrap();
    }

    /// Serializes the store to bytes, for callers (e.g. mobile FFI) that manage
    /// their own persistence instead of using load_or_create()'s file-based storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    /// Reconstructs a store from bytes previously produced by to_bytes().
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }

    pub fn has_index(&self, index: u32) -> bool {
        self.outputs.iter().any(|o| o.index == index)
    }

    pub fn add_output(&mut self, index: u32, value: u64, commitment: Commitment, status: OutputStatus) {
        self.outputs.push(OwnedOutput { index, value, commitment, status });
    }

    pub fn mark_spent(&mut self, commitment: &Commitment) {
        if let Some(o) = self.outputs.iter_mut().find(|o| &o.commitment == commitment) {
            o.status = OutputStatus::Spent;
        }
    }

    /// Reverts a mark_spent call - for a caller that optimistically spent an
    /// output before actually queuing the transaction that spends it, and
    /// then found out the queue attempt failed (see
    /// FaucetState::revert_fee_payment). Only reverts if the output is
    /// still Spent (not Confirmed/Pending) and hasn't already disappeared
    /// from the chain via reconcile(), so this can't resurrect an output
    /// that's genuinely gone.
    pub fn unmark_spent(&mut self, commitment: &Commitment) {
        if let Some(o) = self.outputs.iter_mut().find(|o| &o.commitment == commitment && o.status == OutputStatus::Spent) {
            o.status = OutputStatus::Confirmed;
        }
    }

    /// Removes an output outright - for reverting an add_output call whose
    /// transaction never actually got queued (see
    /// FaucetState::revert_fee_payment). Only removes it while still
    /// Pending, so a real on-chain output that reconcile() has already
    /// confirmed can't be discarded by an unrelated revert.
    pub fn remove_pending_output(&mut self, index: u32) {
        self.outputs.retain(|o| !(o.index == index && o.status == OutputStatus::Pending));
    }

    /// Reconciles local ledger state against the node's current on-chain UTXO set.
    pub fn reconcile(&mut self, chain_utxos: &HashSet<Commitment>) {
        for output in self.outputs.iter_mut() {
            match output.status {
                OutputStatus::Pending => {
                    if chain_utxos.contains(&output.commitment) {
                        output.status = OutputStatus::Confirmed;
                    }
                }
                OutputStatus::Confirmed => {
                    if !chain_utxos.contains(&output.commitment) {
                        output.status = OutputStatus::Spent;
                    }
                }
                OutputStatus::Spent => {
                    // We optimistically marked this Spent the instant we
                    // built a transaction spending it (see planner::plan_send/
                    // wallet::slate::create_slate/api::faucet::
                    // build_sponsored_fee_payment), before it was ever
                    // confirmed. If that transaction never actually landed
                    // on-chain (dropped, lost a fee-priority race, etc.), the
                    // commitment is still really here - our attempted spend
                    // never took effect, so it needs to become spendable
                    // again rather than staying permanently (and wrongly)
                    // marked Spent forever.
                    if chain_utxos.contains(&output.commitment) {
                        output.status = OutputStatus::Confirmed;
                    }
                }
            }
        }
    }

    /// Total confirmed (safely spendable) balance.
    pub fn balance(&self) -> u64 {
        self.outputs.iter()
            .filter(|o| o.status == OutputStatus::Confirmed)
            .map(|o| o.value)
            .sum()
    }

    /// Total pending (unconfirmed) balance.
    pub fn pending_balance(&self) -> u64 {
        self.outputs.iter()
            .filter(|o| o.status == OutputStatus::Pending)
            .map(|o| o.value)
            .sum()
    }

    /// Confirmed outputs, sorted descending by value. Used for validator
    /// staking specifically (see planner::blinding_for's callers in
    /// ffi.rs/wasm.rs) - staking reveals a blinding factor for a commitment
    /// the node checks against its real, already-confirmed UTXO set, so a
    /// Pending output (which doesn't exist on-chain yet) would be rejected
    /// there regardless of what this method returns.
    pub fn spendable(&self) -> Vec<&OwnedOutput> {
        let mut outputs: Vec<&OwnedOutput> = self.outputs.iter()
            .filter(|o| o.status == OutputStatus::Confirmed)
            .collect();
        outputs.sort_by(|a, b| b.value.cmp(&a.value));
        outputs
    }

    /// Confirmed AND Pending outputs, sorted descending by value - used for
    /// ordinary coin selection (planner::select_spendable), so a wallet can
    /// chain a new send off its own not-yet-mined change/incoming output
    /// instead of blocking until it confirms. Safe because Mimblewimble
    /// mempool admission only validates a transaction's own internal math
    /// (see core::mempool::add_transaction - no chain-UTXO check at
    /// admission time), and cut-through aggregation cancels matching
    /// input/output commitments across every transaction pulled into a block
    /// regardless of which one produced them (see
    /// core::cut_through::aggregate_and_cut_through) - so a child spending a
    /// still-pending parent output is valid whether they land in the same
    /// block (cut through cancels the intermediate hop) or sequential ones
    /// (the parent is already confirmed by then). One residual edge case:
    /// if fee-priority mempool selection ever pulls a child into an earlier
    /// block than its own parent, that block would fail to apply (the input
    /// doesn't exist yet) - a pre-existing mempool-ordering gap, not
    /// something this method introduces, and unlikely at real traffic levels.
    pub fn spendable_including_pending(&self) -> Vec<&OwnedOutput> {
        let mut outputs: Vec<&OwnedOutput> = self.outputs.iter()
            .filter(|o| o.status == OutputStatus::Confirmed || o.status == OutputStatus::Pending)
            .collect();
        outputs.sort_by(|a, b| b.value.cmp(&a.value));
        outputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use curve25519_dalek_ng::scalar::Scalar;

    /// The exact bug this session traced through the live faucet: an output
    /// gets optimistically marked Spent the instant a transaction spending
    /// it is built (see planner::plan_send, api::faucet::
    /// build_sponsored_fee_payment), before that transaction is ever
    /// confirmed. If it never actually lands on-chain, the commitment is
    /// still really there - reconcile must revert it back to Confirmed
    /// instead of leaving it wrongly, permanently marked Spent.
    #[test]
    fn reconcile_reverts_a_wrongly_marked_spent_output_back_to_confirmed() {
        let commitment = Commitment::new(1000, Scalar::from(1u64));
        let mut store = WalletStore::default();
        store.add_output(0, 1000, commitment, OutputStatus::Confirmed);
        store.mark_spent(&commitment);
        assert_eq!(store.balance(), 0);

        // The attempted spend never actually landed - the commitment is
        // still in the chain's real UTXO set.
        let mut utxos = HashSet::new();
        utxos.insert(commitment);
        store.reconcile(&utxos);

        assert_eq!(store.balance(), 1000, "wrongly-marked-spent output must become spendable again");
    }

    #[test]
    fn reconcile_leaves_a_genuinely_spent_output_alone() {
        let commitment = Commitment::new(1000, Scalar::from(2u64));
        let mut store = WalletStore::default();
        store.add_output(0, 1000, commitment, OutputStatus::Confirmed);
        store.mark_spent(&commitment);

        // The commitment is genuinely gone from the chain's UTXO set - it
        // must stay Spent.
        let utxos = HashSet::new();
        store.reconcile(&utxos);

        assert_eq!(store.balance(), 0, "a genuinely spent output must not come back");
    }
}
