//! Mobile-facing FFI surface (UniFFI). Keystore/WalletStore cross this boundary as
//! opaque serialized byte blobs - callers (e.g. an Android app) decide how to persist
//! them securely (Android Keystore / EncryptedSharedPreferences) rather than this
//! crate doing file I/O, which isn't the idiomatic mobile pattern. Networking is
//! deliberately excluded from this layer too: callers submit `transaction_json`
//! themselves (plain HTTP), keeping TLS-on-mobile concerns entirely out of this crate.

use std::collections::HashSet;

use crate::crypto::pedersen::Commitment;
use crate::wallet::keystore::Keystore;
use crate::wallet::store::{WalletStore, OutputStatus};
use crate::wallet::planner::{self, PlanError};

#[derive(uniffi::Record)]
pub struct FfiOwnedOutput {
    pub index: u32,
    pub value: u64,
    pub commitment_hex: String,
}

#[derive(uniffi::Record)]
pub struct FfiSendPlan {
    pub transaction_json: String,
    pub updated_keystore_bytes: Vec<u8>,
    pub dest: FfiOwnedOutput,
    pub change: Option<FfiOwnedOutput>,
    pub spent_commitments_hex: Vec<String>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("insufficient balance: have {have}, need {need}")]
    InsufficientBalance { have: u64, need: u64 },
    #[error("invalid keystore bytes")]
    InvalidKeystore,
    #[error("invalid wallet store bytes")]
    InvalidStore,
    #[error("invalid commitment hex: {0}")]
    InvalidCommitment(String),
    #[error("failed to serialize transaction")]
    SerializationFailed,
}

/// Generates a fresh keystore (random seed) and returns its serialized bytes.
#[uniffi::export]
pub fn generate_keystore() -> Vec<u8> {
    Keystore::generate().to_bytes()
}

/// Creates an empty wallet store and returns its serialized bytes.
#[uniffi::export]
pub fn wallet_store_new() -> Vec<u8> {
    WalletStore::default().to_bytes()
}

/// Reconciles a wallet store's local ledger against the node's current on-chain
/// UTXO set (as returned by GET /v1/utxos, hex-encoded), returning updated bytes.
#[uniffi::export]
pub fn reconcile_wallet_store(store_bytes: Vec<u8>, chain_utxo_commitments_hex: Vec<String>) -> Result<Vec<u8>, FfiError> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;
    let mut set = HashSet::new();
    for hex in &chain_utxo_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| FfiError::InvalidCommitment(hex.clone()))?;
        set.insert(commitment);
    }
    store.reconcile(&set);
    Ok(store.to_bytes())
}

/// Confirmed (safely spendable) balance.
#[uniffi::export]
pub fn wallet_balance(store_bytes: Vec<u8>) -> Result<u64, FfiError> {
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;
    Ok(store.balance())
}

/// Pending (unconfirmed) balance.
#[uniffi::export]
pub fn wallet_pending_balance(store_bytes: Vec<u8>) -> Result<u64, FfiError> {
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;
    Ok(store.pending_balance())
}

/// Builds a real, self-contained transaction from the wallet's own confirmed UTXOs.
/// Allocates and persists new output indices in the returned keystore bytes
/// immediately (same as the desktop wallet), regardless of whether the caller
/// goes on to broadcast successfully. The caller must POST `transaction_json`
/// itself, then call `commit_send` only on a successful response.
#[uniffi::export]
pub fn plan_send_ffi(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, amount: u64, fee: u64) -> Result<FfiSendPlan, FfiError> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    let plan = planner::plan_send(&mut keystore, &store, amount, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => FfiError::InsufficientBalance { have, need },
        })?;

    let transaction_json = serde_json::to_string(&plan.transaction).map_err(|_| FfiError::SerializationFailed)?;

    let (dest_index, dest_commitment, dest_value) = plan.dest;
    let dest = FfiOwnedOutput { index: dest_index, value: dest_value, commitment_hex: dest_commitment.to_hex() };
    let change = plan.change.map(|(index, commitment, value)| FfiOwnedOutput {
        index,
        value,
        commitment_hex: commitment.to_hex(),
    });
    let spent_commitments_hex = plan.spent_commitments.iter().map(|c| c.to_hex()).collect();

    Ok(FfiSendPlan {
        transaction_json,
        updated_keystore_bytes: keystore.to_bytes(),
        dest,
        change,
        spent_commitments_hex,
    })
}

/// Applies a previously-built SendPlan's effects to the wallet store. Must only be
/// called after the transaction was successfully broadcast.
#[uniffi::export]
pub fn commit_send(
    store_bytes: Vec<u8>,
    spent_commitments_hex: Vec<String>,
    dest: FfiOwnedOutput,
    change: Option<FfiOwnedOutput>,
) -> Result<Vec<u8>, FfiError> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    for hex in &spent_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| FfiError::InvalidCommitment(hex.clone()))?;
        store.mark_spent(&commitment);
    }

    let dest_commitment = Commitment::from_hex(&dest.commitment_hex)
        .ok_or_else(|| FfiError::InvalidCommitment(dest.commitment_hex.clone()))?;
    store.add_output(dest.index, dest.value, dest_commitment, OutputStatus::Pending);

    if let Some(change) = change {
        let change_commitment = Commitment::from_hex(&change.commitment_hex)
            .ok_or_else(|| FfiError::InvalidCommitment(change.commitment_hex.clone()))?;
        store.add_output(change.index, change.value, change_commitment, OutputStatus::Pending);
    }

    Ok(store.to_bytes())
}
