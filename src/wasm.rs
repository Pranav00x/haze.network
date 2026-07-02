//! Browser-facing surface (wasm-bindgen), mirroring src/ffi.rs's mobile UniFFI
//! surface but for a web wallet. Same design: Keystore/WalletStore cross the
//! boundary as opaque serialized byte blobs - the JS side decides how to
//! persist them (this crate does no storage/encryption itself; the web
//! wallet's UI is responsible for e.g. password-encrypting the keystore
//! bytes before putting them in localStorage). Networking is deliberately
//! excluded here too: the JS side submits `transaction_json` itself via
//! `fetch()`, keeping HTTP entirely out of this crate.

use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use curve25519_dalek_ng::scalar::Scalar;

use crate::crypto::pedersen::Commitment;
use crate::wallet::keystore::Keystore;
use crate::wallet::store::{WalletStore, OutputStatus, GENESIS_INDEX};
use crate::wallet::planner::{self, PlanError};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmOwnedOutput {
    pub index: u32,
    pub value: u64,
    pub commitment_hex: String,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmSendPlan {
    pub transaction_json: String,
    pub updated_keystore_bytes: Vec<u8>,
    pub dest: WasmOwnedOutput,
    pub change: Option<WasmOwnedOutput>,
    pub spent_commitments_hex: Vec<String>,
}

fn js_err(msg: impl Into<String>) -> JsValue {
    JsValue::from_str(&msg.into())
}

/// Generates a fresh keystore (random seed, via the browser's crypto.getRandomValues
/// through getrandom's "js" feature) and returns its serialized bytes.
#[wasm_bindgen]
pub fn generate_keystore() -> Vec<u8> {
    Keystore::generate().to_bytes()
}

/// Creates an empty wallet store and returns its serialized bytes.
#[wasm_bindgen]
pub fn wallet_store_new() -> Vec<u8> {
    WalletStore::default().to_bytes()
}

/// Seeds the store with the well-known devnet genesis output (1,000,000,
/// blinding=42) - devnet-only convenience for funding a fresh web wallet,
/// mirrors the CLI's --claim-genesis. Only one wallet instance should do this.
#[wasm_bindgen]
pub fn claim_genesis(store_bytes: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;
    if !store.has_index(GENESIS_INDEX) {
        let genesis_blinding = Scalar::from(42u64);
        let genesis_value = 1_000_000u64;
        let commitment = Commitment::new(genesis_value, genesis_blinding);
        store.add_output(GENESIS_INDEX, genesis_value, commitment, OutputStatus::Confirmed);
    }
    Ok(store.to_bytes())
}

/// Reconciles a wallet store's local ledger against the node's current on-chain
/// UTXO set (as returned by GET /v1/utxos, hex-encoded), returning updated bytes.
#[wasm_bindgen]
pub fn reconcile_wallet_store(store_bytes: Vec<u8>, chain_utxo_commitments_hex: Vec<String>) -> Result<Vec<u8>, JsValue> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;
    let mut set = HashSet::new();
    for hex in &chain_utxo_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| js_err(format!("invalid commitment hex: {}", hex)))?;
        set.insert(commitment);
    }
    store.reconcile(&set);
    Ok(store.to_bytes())
}

/// Confirmed (safely spendable) balance.
#[wasm_bindgen]
pub fn wallet_balance(store_bytes: Vec<u8>) -> Result<u64, JsValue> {
    let store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;
    Ok(store.balance())
}

/// Pending (unconfirmed) balance.
#[wasm_bindgen]
pub fn wallet_pending_balance(store_bytes: Vec<u8>) -> Result<u64, JsValue> {
    let store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;
    Ok(store.pending_balance())
}

/// Builds a real, self-contained transaction from the wallet's own confirmed UTXOs.
/// Allocates new output indices in the returned keystore bytes immediately (same
/// as the desktop wallet), regardless of whether the caller goes on to broadcast
/// successfully. The caller must POST `transaction_json` itself, then call
/// `commit_send` only on a successful response.
#[wasm_bindgen]
pub fn plan_send(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, amount: u64, fee: u64) -> Result<WasmSendPlan, JsValue> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;

    let plan = planner::plan_send(&mut keystore, &store, amount, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => {
                js_err(format!("insufficient balance: have {}, need {}", have, need))
            }
        })?;

    let transaction_json = serde_json::to_string(&plan.transaction).map_err(|_| js_err("failed to serialize transaction"))?;

    let (dest_index, dest_commitment, dest_value) = plan.dest;
    let dest = WasmOwnedOutput { index: dest_index, value: dest_value, commitment_hex: dest_commitment.to_hex() };
    let change = plan.change.map(|(index, commitment, value)| WasmOwnedOutput {
        index,
        value,
        commitment_hex: commitment.to_hex(),
    });
    let spent_commitments_hex = plan.spent_commitments.iter().map(|c| c.to_hex()).collect();

    Ok(WasmSendPlan {
        transaction_json,
        updated_keystore_bytes: keystore.to_bytes(),
        dest,
        change,
        spent_commitments_hex,
    })
}

/// Applies a previously-built SendPlan's effects to the wallet store. Must only be
/// called after the transaction was successfully broadcast.
#[wasm_bindgen]
pub fn commit_send(
    store_bytes: Vec<u8>,
    spent_commitments_hex: Vec<String>,
    dest: WasmOwnedOutput,
    change: Option<WasmOwnedOutput>,
) -> Result<Vec<u8>, JsValue> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;

    for hex in &spent_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| js_err(format!("invalid commitment hex: {}", hex)))?;
        store.mark_spent(&commitment);
    }

    let dest_commitment = Commitment::from_hex(&dest.commitment_hex)
        .ok_or_else(|| js_err(format!("invalid commitment hex: {}", dest.commitment_hex)))?;
    store.add_output(dest.index, dest.value, dest_commitment, OutputStatus::Pending);

    if let Some(change) = change {
        let change_commitment = Commitment::from_hex(&change.commitment_hex)
            .ok_or_else(|| js_err(format!("invalid commitment hex: {}", change.commitment_hex)))?;
        store.add_output(change.index, change.value, change_commitment, OutputStatus::Pending);
    }

    Ok(store.to_bytes())
}
