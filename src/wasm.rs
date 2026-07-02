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
use crate::wallet::slate::{self, PendingSlate, Slate};

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

// ---------- two-party ("slate") payments, mirroring src/wallet/cli.rs's
// pay/receive/complete - see src/wallet/slate.rs for the protocol itself.
// Slates cross the JS boundary as JSON (to hand to the other party, out of
// band); PendingSlate crosses as opaque bytes (JS persists it locally, same
// pattern as Keystore/WalletStore, until `finalize_slate` consumes it).

#[wasm_bindgen(getter_with_clone)]
pub struct WasmCreateSlateResult {
    /// Hand this to the recipient (out-of-band: chat, email, QR, etc).
    pub slate_json: String,
    /// Keep this locally - never share it. Required by `finalize_slate` later.
    pub pending_slate_bytes: Vec<u8>,
    pub updated_keystore_bytes: Vec<u8>,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmRespondResult {
    /// Send this back to the original sender.
    pub response_slate_json: String,
    pub receiver_output: WasmOwnedOutput,
    pub updated_keystore_bytes: Vec<u8>,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmFinalizedTx {
    pub transaction_json: String,
    pub spent_commitments_hex: Vec<String>,
    pub change: Option<WasmOwnedOutput>,
}

/// Sender step 1: builds a slate paying a different wallet `amount`. Returns
/// the slate JSON to hand to the recipient and the private pending-slate
/// bytes to keep locally until `finalize_slate`.
#[wasm_bindgen]
pub fn create_send_slate(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, amount: u64, fee: u64) -> Result<WasmCreateSlateResult, JsValue> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;

    let (built_slate, pending) = slate::create_slate(&mut keystore, &store, amount, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => {
                js_err(format!("insufficient balance: have {}, need {}", have, need))
            }
        })?;

    let slate_json = serde_json::to_string(&built_slate).map_err(|_| js_err("failed to serialize slate"))?;
    let pending_slate_bytes = bincode::serialize(&pending).map_err(|_| js_err("failed to serialize pending slate"))?;

    Ok(WasmCreateSlateResult {
        slate_json,
        pending_slate_bytes,
        updated_keystore_bytes: keystore.to_bytes(),
    })
}

/// Receiver step: fills in a slate received from a sender. Returns the
/// response JSON to send back, plus the output info the caller should add
/// to its own store as Pending.
#[wasm_bindgen]
pub fn respond_to_slate(keystore_bytes: Vec<u8>, slate_json: String) -> Result<WasmRespondResult, JsValue> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let incoming: Slate = serde_json::from_str(&slate_json).map_err(|_| js_err("invalid slate JSON"))?;

    let (response, owned_output) = slate::respond_to_slate(&mut keystore, &incoming);
    let response_slate_json = serde_json::to_string(&response).map_err(|_| js_err("failed to serialize response slate"))?;
    let receiver_output = WasmOwnedOutput {
        index: owned_output.index,
        value: owned_output.value,
        commitment_hex: owned_output.commitment.to_hex(),
    };

    Ok(WasmRespondResult {
        response_slate_json,
        receiver_output,
        updated_keystore_bytes: keystore.to_bytes(),
    })
}

/// Sender step 2 (final): combines the local pending slate with the
/// recipient's response into the final Transaction. The caller must POST
/// `transaction_json` itself, then call `commit_slate_send` only on success.
#[wasm_bindgen]
pub fn finalize_slate(pending_slate_bytes: Vec<u8>, response_slate_json: String) -> Result<WasmFinalizedTx, JsValue> {
    let pending: PendingSlate = bincode::deserialize(&pending_slate_bytes).map_err(|_| js_err("invalid pending slate bytes"))?;
    let response: Slate = serde_json::from_str(&response_slate_json).map_err(|_| js_err("invalid response slate JSON"))?;

    let transaction = slate::finalize_slate(&pending, &response)
        .map_err(|_| js_err("incomplete response - has the recipient responded to this slate yet?"))?;
    let transaction_json = serde_json::to_string(&transaction).map_err(|_| js_err("failed to serialize transaction"))?;

    let spent_commitments_hex = pending.spent_commitments.iter().map(|c| c.to_hex()).collect();
    let change = pending.change.as_ref().map(|c| WasmOwnedOutput {
        index: c.index,
        value: c.value,
        commitment_hex: c.output.commitment.to_hex(),
    });

    Ok(WasmFinalizedTx { transaction_json, spent_commitments_hex, change })
}

/// Receiver-side commit: adds the output from `respond_to_slate` to the
/// store as Pending. Optimistic (same tradeoff as the CLI) - there's no
/// callback confirming the sender actually broadcasts, so this is applied
/// right after responding rather than after on-chain confirmation.
#[wasm_bindgen]
pub fn commit_receive(store_bytes: Vec<u8>, output: WasmOwnedOutput) -> Result<Vec<u8>, JsValue> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;
    let commitment = Commitment::from_hex(&output.commitment_hex)
        .ok_or_else(|| js_err(format!("invalid commitment hex: {}", output.commitment_hex)))?;
    store.add_output(output.index, output.value, commitment, OutputStatus::Pending);
    Ok(store.to_bytes())
}

/// Sender-side commit: applies a finalized+broadcast slate payment's effects
/// (spent inputs, optional change) to the store. Must only be called after
/// the transaction was successfully broadcast.
#[wasm_bindgen]
pub fn commit_slate_send(store_bytes: Vec<u8>, spent_commitments_hex: Vec<String>, change: Option<WasmOwnedOutput>) -> Result<Vec<u8>, JsValue> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;

    for hex in &spent_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| js_err(format!("invalid commitment hex: {}", hex)))?;
        store.mark_spent(&commitment);
    }

    if let Some(change) = change {
        let change_commitment = Commitment::from_hex(&change.commitment_hex)
            .ok_or_else(|| js_err(format!("invalid commitment hex: {}", change.commitment_hex)))?;
        store.add_output(change.index, change.value, change_commitment, OutputStatus::Pending);
    }

    Ok(store.to_bytes())
}

// ---------- validator staking ----------
// Registering as a validator (POST /v1/stake) doesn't spend the output - it
// just proves ownership by revealing the output's blinding factor to the
// node, so the wallet's own store needs no update afterward (the staked
// output stays Confirmed and still spendable). This does mean the blinding
// travels over the wire to the node, same tradeoff the CLI's `haze stake`
// already has - not something new introduced here.

#[derive(serde::Serialize)]
struct StakeRequestJson {
    commitment: Commitment,
    value: u64,
    blinding: Scalar,
}

/// Builds a POST /v1/stake request body by staking the wallet's single
/// largest confirmed output. Fails if there is no confirmed output at least
/// `min_value`. Does not touch the store - staking doesn't spend anything.
#[wasm_bindgen]
pub fn build_stake_request(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, min_value: u64) -> Result<String, JsValue> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;

    let largest = store.spendable().into_iter().next()
        .ok_or_else(|| js_err("no confirmed balance available to stake"))?;
    if largest.value < min_value {
        return Err(js_err(format!("largest owned output ({}) is below the requested minimum stake ({})", largest.value, min_value)));
    }

    let blinding = planner::blinding_for(&keystore, largest.index);
    let req = StakeRequestJson { commitment: largest.commitment, value: largest.value, blinding };
    serde_json::to_string(&req).map_err(|_| js_err("failed to serialize stake request"))
}

/// Reveals the raw blinding factor (as hex) for the wallet's single largest
/// confirmed output - the private key needed to actually run a node as the
/// proposer for that staked output (`haze node --stake-key <hex>`). This is
/// sensitive: it's the spending key for that output, not just a view key.
/// Only exposed so a wallet holder can run their own validator; never sent
/// anywhere except directly into the user's own node process.
#[wasm_bindgen]
pub fn reveal_stake_blinding_hex(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, min_value: u64) -> Result<String, JsValue> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;

    let largest = store.spendable().into_iter().next()
        .ok_or_else(|| js_err("no confirmed balance available to stake"))?;
    if largest.value < min_value {
        return Err(js_err(format!("largest owned output ({}) is below the requested minimum stake ({})", largest.value, min_value)));
    }

    let blinding = planner::blinding_for(&keystore, largest.index);
    Ok(blinding.to_bytes().iter().map(|b| format!("{:02x}", b)).collect())
}
