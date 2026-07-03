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
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use crate::core::transaction::{Transaction, Input, Output, TxKernel};
use crate::core::registry::{RegisterNameOp, TransferNameOp, NAME_REGISTRATION_FEE, validate_name};
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

#[wasm_bindgen(getter_with_clone)]
pub struct WasmKeystoreAndMnemonic {
    pub keystore_bytes: Vec<u8>,
    /// Only ever available right here at generation time - the keystore
    /// itself never stores or re-derives it. The caller is responsible for
    /// showing it to the user and requiring confirmation it's been saved.
    pub mnemonic: String,
}

/// Generates a fresh keystore backed by a real 12-word BIP39 mnemonic, so it
/// can be recovered later via restore_keystore_from_mnemonic().
#[wasm_bindgen]
pub fn generate_keystore_with_mnemonic() -> WasmKeystoreAndMnemonic {
    let (keystore, mnemonic) = Keystore::generate_with_mnemonic();
    WasmKeystoreAndMnemonic { keystore_bytes: keystore.to_bytes(), mnemonic }
}

/// Reconstructs a keystore from a previously-generated BIP39 phrase.
#[wasm_bindgen]
pub fn restore_keystore_from_mnemonic(phrase: String) -> Result<Vec<u8>, JsValue> {
    Keystore::from_mnemonic(&phrase)
        .map(|k| k.to_bytes())
        .ok_or_else(|| js_err("invalid recovery phrase"))
}

#[derive(serde::Deserialize)]
struct ScanEntry {
    commitment_hex: String,
    note_hex: String,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmRecoveryResult {
    pub keystore_bytes: Vec<u8>,
    pub store_bytes: Vec<u8>,
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

/// Recovers a restored wallet's balance by trying to decrypt every note the
/// node hands back from GET /v1/scan-outputs (see api::explorer::
/// handle_scan_outputs and wallet::note) - a fresh restore has no local
/// record of which on-chain outputs are its own or what they're worth, since
/// a Pedersen commitment hides value and there's no local WalletStore left.
/// Only notes that decrypt successfully under this keystore's own note_key
/// AND are still present in `chain_utxo_commitments_hex` (i.e. unspent) are
/// added back as Confirmed - decrypting is already strong proof of
/// ownership (ChaCha20-Poly1305's auth tag), but the commitment is
/// recomputed from the recovered (index, value) as a final sanity check
/// before trusting it.
#[wasm_bindgen]
pub fn recover_wallet_from_chain(
    keystore_bytes: Vec<u8>,
    scan_entries_json: String,
    chain_utxo_commitments_hex: Vec<String>,
) -> Result<WasmRecoveryResult, JsValue> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let entries: Vec<ScanEntry> = serde_json::from_str(&scan_entries_json).map_err(|_| js_err("invalid scan entries JSON"))?;
    let utxo_set: HashSet<String> = chain_utxo_commitments_hex.into_iter().collect();
    let note_key = keystore.note_key();

    let mut store = WalletStore::default();
    let mut max_index_seen: Option<u32> = None;
    let mut recovered_balance: u64 = 0;
    let mut recovered_count: u32 = 0;

    for entry in &entries {
        let Some(note_bytes) = hex_decode(&entry.note_hex) else { continue };
        let Some((index, value)) = crate::wallet::note::open(&note_key, &note_bytes) else { continue };

        // Sanity check: the recovered (index, value) must actually reproduce
        // this exact on-chain commitment - guards against a corrupted or
        // truncated note that happened to still pass the AEAD tag check.
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

    Ok(WasmRecoveryResult {
        keystore_bytes: keystore.to_bytes(),
        store_bytes: store.to_bytes(),
        recovered_count,
        recovered_balance,
    })
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

// ---------- Haze Naming Registry ----------
// Registration rides a normal, self-contained fee-paying transaction (see
// core::registry::RegisterNameOp) reusing the same coin-selection logic as
// plan_send - the only new piece is the name/signature/owner_pubkey, signed
// with the wallet's stable identity key (Keystore::identity_key), not any
// per-output blinding.

#[wasm_bindgen(getter_with_clone)]
pub struct WasmRegisterNameResult {
    /// POST this to /v1/names/register.
    pub op_json: String,
    pub updated_keystore_bytes: Vec<u8>,
    pub spent_commitments_hex: Vec<String>,
    pub change: Option<WasmOwnedOutput>,
}

/// Builds a RegisterNameOp paying the registration fee from the wallet's own
/// confirmed UTXOs, signed with this wallet's stable naming identity key
/// (the same key every time - so `owner_pubkey` is consistent across
/// registrations from this wallet). The caller must POST `op_json`
/// themselves, then call `commit_register_name` only on success.
#[wasm_bindgen]
pub fn build_register_name_request(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, name: String) -> Result<WasmRegisterNameResult, JsValue> {
    validate_name(&name).map_err(|e| js_err(format!("invalid name: {:?}", e)))?;

    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or_else(|| js_err("invalid wallet store bytes"))?;

    let selected = planner::select_spendable(&store, NAME_REGISTRATION_FEE)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => js_err(format!("insufficient balance: have {}, need {}", have, need)),
        })?;
    let selected_total: u64 = selected.iter().map(|(_, _, value)| value).sum();

    let mut input_blindings: Vec<Scalar> = Vec::new();
    let mut inputs: Vec<Input> = Vec::new();
    let mut spent_commitments_hex: Vec<String> = Vec::new();
    for (index, commitment, _value) in &selected {
        input_blindings.push(planner::blinding_for(&keystore, *index));
        inputs.push(Input { commitment: *commitment });
        spent_commitments_hex.push(commitment.to_hex());
    }

    let change_value = selected_total - NAME_REGISTRATION_FEE;
    let (outputs, change, change_blinding) = if change_value > 0 {
        let change_index = keystore.allocate_index();
        let change_blinding = keystore.derive_blinding(change_index);
        let change_commitment = Commitment::new(change_value, change_blinding);
        let change_proof = RangeProof::prove(change_value, &change_blinding);
        let change_note = crate::wallet::note::seal(&keystore.note_key(), change_index, change_value);
        let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };
        let change_info = WasmOwnedOutput { index: change_index, value: change_value, commitment_hex: change_commitment.to_hex() };
        (vec![output], Some(change_info), change_blinding)
    } else {
        (vec![], None, Scalar::zero())
    };

    let sum_input_blinding: Scalar = input_blindings.iter().sum();
    let excess_r = sum_input_blinding - change_blinding;
    let fee_payment = Transaction {
        inputs,
        outputs,
        kernels: vec![TxKernel {
            excess: Commitment::new(0, excess_r),
            fee: NAME_REGISTRATION_FEE,
            signature: Signature::sign(&NAME_REGISTRATION_FEE.to_le_bytes(), &excess_r),
        }],
    };

    let owner_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
    let signature = RegisterNameOp::sign(&name, &owner_secret);

    let op = RegisterNameOp {
        name,
        owner_pubkey,
        resolves_to: owner_pubkey,
        fee_payment,
        signature,
    };
    let op_json = serde_json::to_string(&op).map_err(|_| js_err("failed to serialize registration"))?;

    Ok(WasmRegisterNameResult {
        op_json,
        updated_keystore_bytes: keystore.to_bytes(),
        spent_commitments_hex,
        change,
    })
}

#[derive(serde::Serialize)]
struct SponsoredRegisterRequestJson {
    name: String,
    owner_pubkey: Commitment,
    resolves_to: Commitment,
    signature: Signature,
}

/// Builds a sponsored registration request body for POST
/// /v1/names/register-sponsored - unlike build_register_name_request, this
/// needs no store/UTXOs/coin-selection at all, since the node's own faucet
/// reserve covers the flat registration fee (see FaucetState::
/// build_sponsored_fee_payment on the server side). This is what lets a
/// brand-new wallet register a name before it has ever received any funds.
#[wasm_bindgen]
pub fn build_sponsored_register_name_request(keystore_bytes: Vec<u8>, name: String) -> Result<String, JsValue> {
    validate_name(&name).map_err(|e| js_err(format!("invalid name: {:?}", e)))?;

    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;

    let owner_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
    let signature = RegisterNameOp::sign(&name, &owner_secret);

    let req = SponsoredRegisterRequestJson { name, owner_pubkey, resolves_to: owner_pubkey, signature };
    serde_json::to_string(&req).map_err(|_| js_err("failed to serialize sponsored registration request"))
}

/// Applies a previously-built name registration's effects (spent inputs,
/// optional change) to the store. Must only be called after the registration
/// was successfully queued via POST /v1/names/register.
#[wasm_bindgen]
pub fn commit_register_name(store_bytes: Vec<u8>, spent_commitments_hex: Vec<String>, change: Option<WasmOwnedOutput>) -> Result<Vec<u8>, JsValue> {
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

/// Derives this wallet's stable naming-registry identity pubkey (hex), so the
/// UI can show "your names resolve to this pubkey" without needing a
/// registration to already exist.
#[wasm_bindgen]
pub fn wallet_identity_pubkey_hex(keystore_bytes: Vec<u8>) -> Result<String, JsValue> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let gens = bulletproofs::PedersenGens::default();
    let pubkey = Commitment(keystore.identity_key() * gens.B_blinding);
    Ok(pubkey.to_hex())
}

/// Builds a TransferNameOp handing a name this wallet currently owns to a
/// new owner/resolution target, signed with this wallet's identity key. No
/// fee, no UTXO involved - the server rejects it if the signature doesn't
/// actually match the name's current on-chain owner. `new_resolves_to_hex`
/// is usually the same as `new_owner_pubkey_hex`, but kept separate to match
/// the underlying protocol (they're allowed to differ).
#[wasm_bindgen]
pub fn build_transfer_name_request(keystore_bytes: Vec<u8>, name: String, new_owner_pubkey_hex: String, new_resolves_to_hex: String) -> Result<String, JsValue> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or_else(|| js_err("invalid keystore bytes"))?;
    let new_owner_pubkey = Commitment::from_hex(&new_owner_pubkey_hex).ok_or_else(|| js_err("invalid new owner pubkey hex"))?;
    let new_resolves_to = Commitment::from_hex(&new_resolves_to_hex).ok_or_else(|| js_err("invalid new resolves-to pubkey hex"))?;

    let current_owner_secret = keystore.identity_key();
    let signature = TransferNameOp::sign(&name, &new_owner_pubkey, &new_resolves_to, &current_owner_secret);

    let op = TransferNameOp { name, new_owner_pubkey, new_resolves_to, signature };
    serde_json::to_string(&op).map_err(|_| js_err("failed to serialize transfer"))
}
