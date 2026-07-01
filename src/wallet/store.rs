use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use serde::{Serialize, Deserialize};

use crate::crypto::pedersen::Commitment;

const WALLET_DIR: &str = "wallet_data";
const STORE_FILE: &str = "wallet_data/utxos.dat";

/// Reserved index used for the well-known genesis output, never allocated by Keystore::allocate_index.
pub const GENESIS_INDEX: u32 = u32::MAX;

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
                OutputStatus::Spent => {}
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

    /// Confirmed outputs, sorted descending by value, suitable for coin selection.
    pub fn spendable(&self) -> Vec<&OwnedOutput> {
        let mut outputs: Vec<&OwnedOutput> = self.outputs.iter()
            .filter(|o| o.status == OutputStatus::Confirmed)
            .collect();
        outputs.sort_by(|a, b| b.value.cmp(&a.value));
        outputs
    }
}
