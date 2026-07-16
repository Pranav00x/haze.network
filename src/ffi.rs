//! Mobile-facing FFI surface (UniFFI), mirroring src/wasm.rs's browser
//! surface feature-for-feature but for a native Android/iOS wallet.
//! Keystore/WalletStore cross this boundary as opaque serialized byte blobs -
//! callers (e.g. an Android app) decide how to persist them securely (Android
//! Keystore / EncryptedSharedPreferences) rather than this crate doing file
//! I/O, which isn't the idiomatic mobile pattern. Networking is deliberately
//! excluded from this layer too: callers submit `transaction_json`/`op_json`
//! themselves (plain HTTP), keeping TLS-on-mobile concerns entirely out of
//! this crate.

use std::collections::HashSet;

use curve25519_dalek_ng::scalar::Scalar;
use haze_crypto::pedersen::Commitment;
use haze_crypto::range_proof::RangeProof;
use haze_crypto::schnorr::Signature;
use haze_chain::transaction::{Transaction, Input, Output, TxKernel};
use haze_chain::registry::{RegisterNameOp, TransferNameOp, NAME_REGISTRATION_FEE, validate_name};
use haze_chain::assets::{MintAssetOp, TransferAssetOp, ASSET_MINT_FEE, validate_asset_id};
use haze_chain::marketplace::{Listing, cancel_signing_message};
use haze_chain::collections::{LaunchCollectionOp, MintPhase, allowlist_leaf};
use haze_chain::merkle::{merkle_root, build_merkle_proof};
use haze_wallet::keystore::Keystore;
use haze_wallet::store::{WalletStore, OutputStatus, GENESIS_INDEX};
use haze_wallet::planner::{self, PlanError};
use haze_wallet::slate::{self, PendingSlate, Slate};

#[derive(uniffi::Record, Clone)]
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
    #[error("invalid recovery phrase")]
    InvalidMnemonic,
    #[error("invalid scan entries JSON")]
    InvalidScanEntries,
    #[error("invalid hex: {0}")]
    InvalidHex(String),
    #[error("no unswept validator rewards found")]
    NoUnsweptRewards,
    #[error("found {total} total rewards, which doesn't cover the fee ({fee})")]
    RewardsBelowFee { total: u64, fee: u64 },
    #[error("invalid slate JSON")]
    InvalidSlateJson,
    #[error("invalid pending slate bytes")]
    InvalidPendingSlate,
    #[error("incomplete response - has the recipient responded to this slate yet?")]
    IncompleteSlateResponse,
    #[error("no confirmed balance available to stake")]
    NoConfirmedBalance,
    #[error("largest owned output ({have}) is below the requested minimum stake ({min})")]
    BelowMinimumStake { have: u64, min: u64 },
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("fee must be at least {0}")]
    FeeBelowRegistrationFloor(u64),
    #[error("internal error: rotate-seed transaction unexpectedly produced change - aborting rather than stranding funds under the old seed")]
    UnexpectedRotateChange,
    #[error("invalid asset id: {0}")]
    InvalidAssetId(String),
    #[error("fee must be at least {0}")]
    FeeBelowAssetMintFloor(u64),
    #[error("metadata must be at most {0} bytes")]
    MetadataTooLarge(u64),
    #[error("failed to serialize mint")]
    MintSerializationFailed,
    #[error("invalid mint op json")]
    InvalidMintOpJson,
    #[error("failed to serialize transfer")]
    TransferSerializationFailed,
    #[error("invalid transaction json")]
    InvalidTransactionJson,
    #[error("transaction has no kernels")]
    TransactionHasNoKernels,
    #[error("failed to serialize listing")]
    ListingSerializationFailed,
    #[error("failed to serialize cancellation")]
    CancellationSerializationFailed,
    #[error("royalty_bps must be at most {0}")]
    RoyaltyTooHigh(u16),
    #[error("invalid phases_json: {0}")]
    InvalidPhasesJson(String),
    #[error("failed to serialize collection launch")]
    CollectionLaunchSerializationFailed,
    #[error("target pubkey is not present in the allowlist")]
    TargetNotInAllowlist,
    #[error("failed to serialize allowlist publish")]
    AllowlistSerializationFailed,
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len()).step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

fn scalar_from_hex(hex: &str) -> Option<Scalar> {
    let bytes = hex_decode(hex)?;
    if bytes.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Some(Scalar::from_bits(arr))
}

/// Generates a fresh keystore (random seed) and returns its serialized bytes.
#[uniffi::export]
pub fn generate_keystore() -> Vec<u8> {
    Keystore::generate().to_bytes()
}

#[derive(uniffi::Record)]
pub struct FfiKeystoreAndMnemonic {
    pub keystore_bytes: Vec<u8>,
    /// Only ever available right here at generation time - the keystore
    /// itself never stores or re-derives it. The caller is responsible for
    /// showing it to the user and requiring confirmation it's been saved.
    pub mnemonic: String,
}

/// Generates a fresh keystore backed by a real 12-word BIP39 mnemonic, so it
/// can be recovered later via restore_keystore_from_mnemonic().
#[uniffi::export]
pub fn generate_keystore_with_mnemonic() -> FfiKeystoreAndMnemonic {
    let (keystore, mnemonic) = Keystore::generate_with_mnemonic();
    FfiKeystoreAndMnemonic { keystore_bytes: keystore.to_bytes(), mnemonic }
}

/// Reconstructs a keystore from a previously-generated BIP39 phrase.
#[uniffi::export]
pub fn restore_keystore_from_mnemonic(phrase: String) -> Result<Vec<u8>, FfiError> {
    Keystore::from_mnemonic(&phrase).map(|k| k.to_bytes()).ok_or(FfiError::InvalidMnemonic)
}

#[derive(serde::Deserialize)]
struct ScanEntry {
    commitment_hex: String,
    note_hex: String,
}

#[derive(uniffi::Record)]
pub struct FfiRecoveryResult {
    pub keystore_bytes: Vec<u8>,
    pub store_bytes: Vec<u8>,
    pub recovered_count: u32,
    pub recovered_balance: u64,
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
#[uniffi::export]
pub fn recover_wallet_from_chain(
    keystore_bytes: Vec<u8>,
    scan_entries_json: String,
    chain_utxo_commitments_hex: Vec<String>,
) -> Result<FfiRecoveryResult, FfiError> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let entries: Vec<ScanEntry> = serde_json::from_str(&scan_entries_json).map_err(|_| FfiError::InvalidScanEntries)?;
    let utxo_set: HashSet<String> = chain_utxo_commitments_hex.into_iter().collect();

    let entries: Vec<haze_wallet::recovery::ScanEntry> = entries.into_iter()
        .map(|e| haze_wallet::recovery::ScanEntry { commitment_hex: e.commitment_hex, note_hex: e.note_hex })
        .collect();
    let result = haze_wallet::recovery::recover_from_chain(&mut keystore, &entries, &utxo_set);

    Ok(FfiRecoveryResult {
        keystore_bytes: keystore.to_bytes(),
        store_bytes: result.store.to_bytes(),
        recovered_count: result.recovered_count,
        recovered_balance: result.recovered_balance,
    })
}

#[derive(uniffi::Record)]
pub struct FfiSweepResult {
    /// POST this to /v1/transactions.
    pub transaction_json: String,
    pub updated_keystore_bytes: Vec<u8>,
    /// Add this to the wallet's own store as Pending on success (reuse
    /// commit_send with an empty spent_commitments_hex and no change - the
    /// swept reward inputs were never part of this wallet's own store to
    /// begin with, only the destination output is new).
    pub dest: FfiOwnedOutput,
    pub swept_count: u32,
    pub swept_total: u64,
}

/// Finds every still-unspent block reward this validator has ever earned
/// (see wallet::note::coinbase_blinding/coinbase_note_key and
/// core::proposer, which derives coinbase blindings from the staking secret
/// instead of a discarded random one) and sweeps all of them into a single
/// new output in this wallet's own keystore - turning "provably mine but
/// nowhere to spend it from" into an ordinary, self-owned, spendable
/// balance. `stake_key_hex` is the same secret reveal_stake_blinding_hex
/// already exposes for running a validator node with. Errors if nothing
/// unswept is found, or if the total found doesn't even cover `fee`.
#[uniffi::export]
pub fn sweep_validator_rewards(
    stake_key_hex: String,
    scan_entries_json: String,
    chain_utxo_commitments_hex: Vec<String>,
    keystore_bytes: Vec<u8>,
    fee: u64,
) -> Result<FfiSweepResult, FfiError> {
    let stake_key = scalar_from_hex(&stake_key_hex).ok_or_else(|| FfiError::InvalidHex(stake_key_hex.clone()))?;
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let entries: Vec<ScanEntry> = serde_json::from_str(&scan_entries_json).map_err(|_| FfiError::InvalidScanEntries)?;
    let utxo_set: HashSet<String> = chain_utxo_commitments_hex.into_iter().collect();
    let note_key = haze_crypto::note::coinbase_note_key(&stake_key);

    let mut inputs: Vec<Input> = Vec::new();
    let mut input_blindings: Vec<Scalar> = Vec::new();
    let mut swept_total: u64 = 0;

    for entry in &entries {
        let Some(note_bytes) = hex_decode(&entry.note_hex) else { continue };
        let Some((height, value)) = haze_crypto::note::open(&note_key, &note_bytes) else { continue };
        if !utxo_set.contains(&entry.commitment_hex) {
            continue;
        }

        let blinding = haze_crypto::note::coinbase_blinding(&stake_key, height as u64);
        let commitment = Commitment::new(value, blinding);
        if commitment.to_hex() != entry.commitment_hex {
            continue;
        }

        inputs.push(Input { commitment });
        input_blindings.push(blinding);
        swept_total += value;
    }

    let swept_count = inputs.len() as u32;
    if swept_count == 0 {
        return Err(FfiError::NoUnsweptRewards);
    }
    if swept_total <= fee {
        return Err(FfiError::RewardsBelowFee { total: swept_total, fee });
    }

    let dest_value = swept_total - fee;
    let dest_index = keystore.allocate_index();
    let dest_blinding = keystore.derive_blinding(dest_index);
    let dest_commitment = Commitment::new(dest_value, dest_blinding);
    let dest_proof = RangeProof::prove(dest_value, &dest_blinding);
    let dest_note = haze_crypto::note::seal(&keystore.note_key(), dest_index, dest_value);
    let dest_output = Output { commitment: dest_commitment, proof: dest_proof, note: dest_note };

    let sum_input_blinding: Scalar = input_blindings.iter().sum();
    let excess_r = sum_input_blinding - dest_blinding;
    let kernel = TxKernel {
        excess: Commitment::new(0, excess_r),
        fee,
        signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
    };

    let transaction = Transaction { inputs, outputs: vec![dest_output], kernels: vec![kernel] };
    let transaction_json = serde_json::to_string(&transaction).map_err(|_| FfiError::SerializationFailed)?;

    Ok(FfiSweepResult {
        transaction_json,
        updated_keystore_bytes: keystore.to_bytes(),
        dest: FfiOwnedOutput { index: dest_index, value: dest_value, commitment_hex: dest_commitment.to_hex() },
        swept_count,
        swept_total,
    })
}

/// Creates an empty wallet store and returns its serialized bytes.
#[uniffi::export]
pub fn wallet_store_new() -> Vec<u8> {
    WalletStore::default().to_bytes()
}

/// Seeds the store with the well-known devnet genesis output (1,000,000,
/// blinding=42) - devnet-only convenience for funding a fresh wallet, mirrors
/// the CLI's --claim-genesis. Only one wallet instance should do this.
#[uniffi::export]
pub fn claim_genesis(store_bytes: Vec<u8>) -> Result<Vec<u8>, FfiError> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;
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

// ---------- seed rotation ----------
// There's no "account" to re-key here the way there would be on an
// account-based chain - owning a coin means knowing its blinding factor,
// which is derived from the seed that sealed it. "Replacing" a seed
// therefore has to be a real on-chain sweep: spend everything the old seed
// owns into fresh outputs owned by a brand-new seed, in one transaction.
// Mirrors src/wasm.rs's rotate_seed_transaction exactly - reuses the
// two-party slate protocol (wallet::slate) driven synchronously end-to-end
// in a single call, since both "parties" (old and new keystore) are known
// locally here.

#[derive(uniffi::Record)]
pub struct FfiRotateSeedResult {
    pub transaction_json: String,
    pub spent_commitments_hex: Vec<String>,
    pub dest: FfiOwnedOutput,
}

/// Builds a transaction sweeping this wallet's entire confirmed balance into
/// a single fresh output owned by `new_keystore_bytes` - generate that via
/// `generate_keystore_with_mnemonic()` first. `dest` is the swept output;
/// the caller should build a *fresh* store for the new keystore and add it
/// as Pending via `commit_send`-style logic (spent_commitments_hex empty,
/// dest is this call's `dest`, no change) once the transaction is confirmed
/// broadcast. The old keystore/store can simply be discarded afterward -
/// nothing is left behind under the old seed by construction (amount is set
/// to exactly balance-minus-fee, so selection must use every confirmed
/// output and change is always zero; see wallet::slate::build_slate).
#[uniffi::export]
pub fn rotate_seed_transaction(old_keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, new_keystore_bytes: Vec<u8>, fee: u64) -> Result<FfiRotateSeedResult, FfiError> {
    let mut old_keystore = Keystore::from_bytes(&old_keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;
    let mut new_keystore = Keystore::from_bytes(&new_keystore_bytes).ok_or(FfiError::InvalidKeystore)?;

    let balance = store.balance();
    let amount = balance.checked_sub(fee)
        .ok_or(FfiError::InsufficientBalance { have: balance, need: fee })?;

    let (built_slate, pending) = slate::create_slate(&mut old_keystore, &store, amount, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => FfiError::InsufficientBalance { have, need },
        })?;

    let (response, receiver_info) = slate::respond_to_slate(&mut new_keystore, &built_slate);
    let tx = slate::finalize_slate(&pending, &response).map_err(|_| FfiError::IncompleteSlateResponse)?;

    // Should be unreachable (amount == balance - fee forces every confirmed
    // output into the selection, leaving nothing for change) - a hard error
    // instead of a silent Some(..) is deliberate: this flow's entire point
    // is that nothing stays behind under the seed being abandoned.
    if pending.change.is_some() {
        return Err(FfiError::UnexpectedRotateChange);
    }

    let transaction_json = serde_json::to_string(&tx).map_err(|_| FfiError::SerializationFailed)?;
    let spent_commitments_hex = pending.spent_commitments.iter().map(|c| c.to_hex()).collect();
    let dest = FfiOwnedOutput {
        index: receiver_info.index,
        value: receiver_info.value,
        commitment_hex: receiver_info.commitment.to_hex(),
    };

    Ok(FfiRotateSeedResult { transaction_json, spent_commitments_hex, dest })
}

// ---------- two-party ("slate") payments, mirroring src/wallet/cli.rs's
// pay/receive/complete - see src/wallet/slate.rs for the protocol itself.
// Slates cross the FFI boundary as JSON (to hand to the other party, out of
// band); PendingSlate crosses as opaque bytes (the app persists it locally,
// same pattern as Keystore/WalletStore, until `finalize_slate` consumes it).

#[derive(uniffi::Record)]
pub struct FfiCreateSlateResult {
    /// Hand this to the recipient (out-of-band: chat, email, QR, etc).
    pub slate_json: String,
    /// Keep this locally - never share it. Required by `finalize_slate` later.
    pub pending_slate_bytes: Vec<u8>,
    pub updated_keystore_bytes: Vec<u8>,
}

#[derive(uniffi::Record)]
pub struct FfiRespondResult {
    /// Send this back to the original sender.
    pub response_slate_json: String,
    pub receiver_output: FfiOwnedOutput,
    pub updated_keystore_bytes: Vec<u8>,
}

#[derive(uniffi::Record)]
pub struct FfiFinalizedTx {
    pub transaction_json: String,
    pub spent_commitments_hex: Vec<String>,
    pub change: Option<FfiOwnedOutput>,
}

/// Sender step 1: builds a slate paying a different wallet `amount`. Returns
/// the slate JSON to hand to the recipient and the private pending-slate
/// bytes to keep locally until `finalize_slate`.
#[uniffi::export]
pub fn create_send_slate(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, amount: u64, fee: u64) -> Result<FfiCreateSlateResult, FfiError> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    let (built_slate, pending) = slate::create_slate(&mut keystore, &store, amount, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => FfiError::InsufficientBalance { have, need },
        })?;

    let slate_json = serde_json::to_string(&built_slate).map_err(|_| FfiError::SerializationFailed)?;
    let pending_slate_bytes = bincode::serialize(&pending).map_err(|_| FfiError::SerializationFailed)?;

    Ok(FfiCreateSlateResult {
        slate_json,
        pending_slate_bytes,
        updated_keystore_bytes: keystore.to_bytes(),
    })
}

/// Receiver step: fills in a slate received from a sender. Returns the
/// response JSON to send back, plus the output info the caller should add
/// to its own store as Pending.
#[uniffi::export]
pub fn respond_to_slate(keystore_bytes: Vec<u8>, slate_json: String) -> Result<FfiRespondResult, FfiError> {
    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let incoming: Slate = serde_json::from_str(&slate_json).map_err(|_| FfiError::InvalidSlateJson)?;

    let (response, owned_output) = slate::respond_to_slate(&mut keystore, &incoming);
    let response_slate_json = serde_json::to_string(&response).map_err(|_| FfiError::SerializationFailed)?;
    let receiver_output = FfiOwnedOutput {
        index: owned_output.index,
        value: owned_output.value,
        commitment_hex: owned_output.commitment.to_hex(),
    };

    Ok(FfiRespondResult {
        response_slate_json,
        receiver_output,
        updated_keystore_bytes: keystore.to_bytes(),
    })
}

/// Sender step 2 (final): combines the local pending slate with the
/// recipient's response into the final Transaction. The caller must POST
/// `transaction_json` itself, then call `commit_slate_send` only on success.
#[uniffi::export]
pub fn finalize_slate(pending_slate_bytes: Vec<u8>, response_slate_json: String) -> Result<FfiFinalizedTx, FfiError> {
    let pending: PendingSlate = bincode::deserialize(&pending_slate_bytes).map_err(|_| FfiError::InvalidPendingSlate)?;
    let response: Slate = serde_json::from_str(&response_slate_json).map_err(|_| FfiError::InvalidSlateJson)?;

    let transaction = slate::finalize_slate(&pending, &response).map_err(|_| FfiError::IncompleteSlateResponse)?;
    let transaction_json = serde_json::to_string(&transaction).map_err(|_| FfiError::SerializationFailed)?;

    let spent_commitments_hex = pending.spent_commitments.iter().map(|c| c.to_hex()).collect();
    let change = pending.change.as_ref().map(|c| FfiOwnedOutput {
        index: c.index,
        value: c.value,
        commitment_hex: c.output.commitment.to_hex(),
    });

    Ok(FfiFinalizedTx { transaction_json, spent_commitments_hex, change })
}

/// Receiver-side commit: adds the output from `respond_to_slate` to the
/// store as Pending. Optimistic (same tradeoff as the CLI) - there's no
/// callback confirming the sender actually broadcasts, so this is applied
/// right after responding rather than after on-chain confirmation.
#[uniffi::export]
pub fn commit_receive(store_bytes: Vec<u8>, output: FfiOwnedOutput) -> Result<Vec<u8>, FfiError> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;
    let commitment = Commitment::from_hex(&output.commitment_hex)
        .ok_or_else(|| FfiError::InvalidCommitment(output.commitment_hex.clone()))?;
    store.add_output(output.index, output.value, commitment, OutputStatus::Pending);
    Ok(store.to_bytes())
}

/// Sender-side commit: applies a finalized+broadcast slate payment's effects
/// (spent inputs, optional change) to the store. Must only be called after
/// the transaction was successfully broadcast.
#[uniffi::export]
pub fn commit_slate_send(store_bytes: Vec<u8>, spent_commitments_hex: Vec<String>, change: Option<FfiOwnedOutput>) -> Result<Vec<u8>, FfiError> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    for hex in &spent_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| FfiError::InvalidCommitment(hex.clone()))?;
        store.mark_spent(&commitment);
    }

    if let Some(change) = change {
        let change_commitment = Commitment::from_hex(&change.commitment_hex)
            .ok_or_else(|| FfiError::InvalidCommitment(change.commitment_hex.clone()))?;
        store.add_output(change.index, change.value, change_commitment, OutputStatus::Pending);
    }

    Ok(store.to_bytes())
}

// ---------- validator staking ----------
// Registering as a validator (POST /v1/stake) doesn't spend the output - the
// wallet signs a proof of ownership locally (see
// core::chain::stake_registration_message/register_validator) rather than
// sending the output's raw blinding factor to the node, so the secret never
// travels over the wire at all - unlike a plain reveal, this proof is also
// safe for the node to re-gossip to its peers.

#[derive(serde::Serialize)]
struct StakeRequestJson {
    commitment: Commitment,
    value: u64,
    proof: Signature,
}

/// Builds a POST /v1/stake request body by staking the wallet's single
/// largest confirmed output. Fails if there is no confirmed output at least
/// `min_value`. Does not touch the store - staking doesn't spend anything.
#[uniffi::export]
pub fn build_stake_request(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, min_value: u64) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    let largest = store.spendable().into_iter().next().ok_or(FfiError::NoConfirmedBalance)?;
    if largest.value < min_value {
        return Err(FfiError::BelowMinimumStake { have: largest.value, min: min_value });
    }

    let blinding = planner::blinding_for(&keystore, largest.index);
    let msg = haze_chain::chain::stake_registration_message(&largest.commitment, largest.value);
    let proof = Signature::sign(&msg, &blinding);
    let req = StakeRequestJson { commitment: largest.commitment, value: largest.value, proof };
    serde_json::to_string(&req).map_err(|_| FfiError::SerializationFailed)
}

/// Reveals the raw blinding factor (as hex) for the wallet's single largest
/// confirmed output - the private key needed to actually run a node as the
/// proposer for that staked output (`haze node --stake-key <hex>`). This is
/// sensitive: it's the spending key for that output, not just a view key.
/// Only exposed so a wallet holder can run their own validator; never sent
/// anywhere except directly into the user's own node process.
#[uniffi::export]
pub fn reveal_stake_blinding_hex(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, min_value: u64) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    let largest = store.spendable().into_iter().next().ok_or(FfiError::NoConfirmedBalance)?;
    if largest.value < min_value {
        return Err(FfiError::BelowMinimumStake { have: largest.value, min: min_value });
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

#[derive(uniffi::Record)]
pub struct FfiRegisterNameResult {
    /// POST this to /v1/names/register.
    pub op_json: String,
    pub updated_keystore_bytes: Vec<u8>,
    pub spent_commitments_hex: Vec<String>,
    pub change: Option<FfiOwnedOutput>,
}

/// Builds a RegisterNameOp paying `fee` (must be >= NAME_REGISTRATION_FEE,
/// the hard consensus floor - see its doc comment for why the floor itself
/// can't be a live congestion-derived value) from the wallet's own confirmed
/// UTXOs, signed with this wallet's stable naming identity key (the same key
/// every time - so `owner_pubkey` is consistent across registrations from
/// this wallet). Callers should pass GET /v1/fee-estimate's
/// suggested_name_fee rather than hardcoding NAME_REGISTRATION_FEE, so
/// paying "the going rate" adapts to how busy the name-registration backlog
/// actually is. The caller must POST `op_json` themselves, then call
/// `commit_register_name` only on success.
#[uniffi::export]
pub fn build_register_name_request(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, name: String, fee: u64) -> Result<FfiRegisterNameResult, FfiError> {
    validate_name(&name).map_err(|e| FfiError::InvalidName(format!("{:?}", e)))?;
    if fee < NAME_REGISTRATION_FEE {
        return Err(FfiError::FeeBelowRegistrationFloor(NAME_REGISTRATION_FEE));
    }

    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    let selected = planner::select_spendable_confirmed_only(&store, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => FfiError::InsufficientBalance { have, need },
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

    let change_value = selected_total - fee;
    let (outputs, change, change_blinding) = if change_value > 0 {
        let change_index = keystore.allocate_index();
        let change_blinding = keystore.derive_blinding(change_index);
        let change_commitment = Commitment::new(change_value, change_blinding);
        let change_proof = RangeProof::prove(change_value, &change_blinding);
        let change_note = haze_crypto::note::seal(&keystore.note_key(), change_index, change_value);
        let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };
        let change_info = FfiOwnedOutput { index: change_index, value: change_value, commitment_hex: change_commitment.to_hex() };
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
            fee,
            signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
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
    let op_json = serde_json::to_string(&op).map_err(|_| FfiError::SerializationFailed)?;

    Ok(FfiRegisterNameResult {
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
#[uniffi::export]
pub fn build_sponsored_register_name_request(keystore_bytes: Vec<u8>, name: String) -> Result<String, FfiError> {
    validate_name(&name).map_err(|e| FfiError::InvalidName(format!("{:?}", e)))?;

    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;

    let owner_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
    let signature = RegisterNameOp::sign(&name, &owner_secret);

    let req = SponsoredRegisterRequestJson { name, owner_pubkey, resolves_to: owner_pubkey, signature };
    serde_json::to_string(&req).map_err(|_| FfiError::SerializationFailed)
}

/// Applies a previously-built name registration's effects (spent inputs,
/// optional change) to the store. Must only be called after the registration
/// was successfully queued via POST /v1/names/register.
#[uniffi::export]
pub fn commit_register_name(store_bytes: Vec<u8>, spent_commitments_hex: Vec<String>, change: Option<FfiOwnedOutput>) -> Result<Vec<u8>, FfiError> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    for hex in &spent_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| FfiError::InvalidCommitment(hex.clone()))?;
        store.mark_spent(&commitment);
    }

    if let Some(change) = change {
        let change_commitment = Commitment::from_hex(&change.commitment_hex)
            .ok_or_else(|| FfiError::InvalidCommitment(change.commitment_hex.clone()))?;
        store.add_output(change.index, change.value, change_commitment, OutputStatus::Pending);
    }

    Ok(store.to_bytes())
}

/// Derives this wallet's stable naming-registry identity pubkey (hex), so the
/// app can show "your names resolve to this pubkey" without needing a
/// registration to already exist.
#[uniffi::export]
pub fn wallet_identity_pubkey_hex(keystore_bytes: Vec<u8>) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
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
#[uniffi::export]
pub fn build_transfer_name_request(keystore_bytes: Vec<u8>, name: String, new_owner_pubkey_hex: String, new_resolves_to_hex: String) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let new_owner_pubkey = Commitment::from_hex(&new_owner_pubkey_hex).ok_or_else(|| FfiError::InvalidHex(new_owner_pubkey_hex.clone()))?;
    let new_resolves_to = Commitment::from_hex(&new_resolves_to_hex).ok_or_else(|| FfiError::InvalidHex(new_resolves_to_hex.clone()))?;

    let current_owner_secret = keystore.identity_key();
    let signature = TransferNameOp::sign(&name, &new_owner_pubkey, &new_resolves_to, &current_owner_secret);

    let op = TransferNameOp { name, new_owner_pubkey, new_resolves_to, signature };
    serde_json::to_string(&op).map_err(|_| FfiError::SerializationFailed)
}

// ---------- plain send planning, identity signing, slate reservations ----------
// These mirror src/wasm.rs exactly but were missing from this mobile surface
// entirely until now - not a design choice, just drift between the two
// bindings surfaces that had gone unnoticed.

/// Builds a plain self-planned send (no two-party handshake) - see
/// wallet::planner::plan_send. The caller must POST `transaction_json`
/// themselves, then call `commit_send` only on success.
#[uniffi::export]
pub fn plan_send(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, amount: u64, fee: u64) -> Result<FfiSendPlan, FfiError> {
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

#[derive(uniffi::Record)]
pub struct FfiSlateReservation {
    pub spent_commitments_hex: Vec<String>,
    pub change: Option<FfiOwnedOutput>,
}

/// Sender-side: the inputs/change a pending slate has ALREADY selected at
/// create_send_slate time, before the recipient has even responded - lets a
/// caller building a SECOND, independent slate off the same wallet store
/// (e.g. a royalty payment alongside a marketplace payment) eagerly
/// commit_slate_send this one's reservation first, so the second selection
/// can't pick the same commitment.
#[uniffi::export]
pub fn pending_slate_reservation(pending_slate_bytes: Vec<u8>) -> Result<FfiSlateReservation, FfiError> {
    let pending: PendingSlate = bincode::deserialize(&pending_slate_bytes).map_err(|_| FfiError::InvalidPendingSlate)?;
    let spent_commitments_hex = pending.spent_commitments.iter().map(|c| c.to_hex()).collect();
    let change = pending.change.as_ref().map(|c| FfiOwnedOutput {
        index: c.index,
        value: c.value,
        commitment_hex: c.output.commitment.to_hex(),
    });
    Ok(FfiSlateReservation { spent_commitments_hex, change })
}

/// Signs an arbitrary UTF-8 message with this wallet's identity key - the
/// same signature scheme the inbox relay (want_transfer/response/etc) and a
/// "connect wallet" handoff use to prove control of an identity pubkey.
#[uniffi::export]
pub fn sign_identity_message(keystore_bytes: Vec<u8>, message: String) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let signature = Signature::sign(message.as_bytes(), &keystore.identity_key());
    Ok(signature.to_hex())
}

/// Verifies a signature produced by sign_identity_message.
#[uniffi::export]
pub fn verify_identity_signature(pubkey_hex: String, message: String, signature_hex: String) -> Result<bool, FfiError> {
    let pubkey = Commitment::from_hex(&pubkey_hex).ok_or_else(|| FfiError::InvalidHex(pubkey_hex.clone()))?;
    let signature = Signature::from_hex(&signature_hex).ok_or_else(|| FfiError::InvalidHex(signature_hex.clone()))?;
    Ok(signature.verify(message.as_bytes(), &pubkey))
}

// ---------- Haze Asset Registry (NFTs) ----------
// Same shape as the naming registry above - a separate namespace, but
// reusing the wallet's stable identity key as the asset owner pubkey too.
// Mint rides a normal fee-paying transaction, same coin-selection as
// plan_send/build_register_name_request; transfer is signature-only, no
// fee/UTXO involved.

#[derive(uniffi::Record)]
pub struct FfiMintAssetResult {
    /// POST this to /v1/assets/mint.
    pub op_json: String,
    pub updated_keystore_bytes: Vec<u8>,
    pub spent_commitments_hex: Vec<String>,
    pub change: Option<FfiOwnedOutput>,
}

/// Builds a MintAssetOp paying `fee` (must be >= ASSET_MINT_FEE) from the
/// wallet's own confirmed UTXOs, signed with this wallet's stable identity
/// key. `metadata` is free-form text (recommended shape: JSON
/// `{title, description, image}`), bounded at MAX_METADATA_BYTES. The
/// caller must POST `op_json` themselves, then call `commit_mint_asset`
/// only on success.
#[uniffi::export]
pub fn build_mint_asset_request(keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, asset_id: String, metadata: String, fee: u64) -> Result<FfiMintAssetResult, FfiError> {
    validate_asset_id(&asset_id).map_err(|e| FfiError::InvalidAssetId(format!("{:?}", e)))?;
    if fee < ASSET_MINT_FEE {
        return Err(FfiError::FeeBelowAssetMintFloor(ASSET_MINT_FEE));
    }
    if metadata.len() > haze_chain::assets::MAX_METADATA_BYTES {
        return Err(FfiError::MetadataTooLarge(haze_chain::assets::MAX_METADATA_BYTES as u64));
    }

    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    let selected = planner::select_spendable_confirmed_only(&store, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => FfiError::InsufficientBalance { have, need },
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

    let change_value = selected_total - fee;
    let (outputs, change, change_blinding) = if change_value > 0 {
        let change_index = keystore.allocate_index();
        let change_blinding = keystore.derive_blinding(change_index);
        let change_commitment = Commitment::new(change_value, change_blinding);
        let change_proof = RangeProof::prove(change_value, &change_blinding);
        let change_note = haze_crypto::note::seal(&keystore.note_key(), change_index, change_value);
        let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };
        let change_info = FfiOwnedOutput { index: change_index, value: change_value, commitment_hex: change_commitment.to_hex() };
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
            fee,
            signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
        }],
    };

    let metadata_bytes = metadata.into_bytes();

    let owner_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
    let signature = MintAssetOp::sign(&asset_id, &metadata_bytes, &None, &None, &None, &owner_secret);

    let op = MintAssetOp {
        asset_id,
        owner_pubkey,
        metadata: metadata_bytes,
        fee_payment,
        collection_id: None,
        phase_index: None,
        allowlist_proof: None,
        allowlist_leaf_index: None,
        required_kernel_excess: None,
        signature,
        creator_signature: None,
    };
    let op_json = serde_json::to_string(&op).map_err(|_| FfiError::MintSerializationFailed)?;

    Ok(FfiMintAssetResult {
        op_json,
        updated_keystore_bytes: keystore.to_bytes(),
        spent_commitments_hex,
        change,
    })
}

/// Applies a previously-built asset mint's effects (spent inputs, optional
/// change) to the store. Must only be called after the mint was successfully
/// queued via POST /v1/assets/mint.
#[uniffi::export]
pub fn commit_mint_asset(store_bytes: Vec<u8>, spent_commitments_hex: Vec<String>, change: Option<FfiOwnedOutput>) -> Result<Vec<u8>, FfiError> {
    let mut store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    for hex in &spent_commitments_hex {
        let commitment = Commitment::from_hex(hex).ok_or_else(|| FfiError::InvalidCommitment(hex.clone()))?;
        store.mark_spent(&commitment);
    }

    if let Some(change) = change {
        let change_commitment = Commitment::from_hex(&change.commitment_hex)
            .ok_or_else(|| FfiError::InvalidCommitment(change.commitment_hex.clone()))?;
        store.add_output(change.index, change.value, change_commitment, OutputStatus::Pending);
    }

    Ok(store.to_bytes())
}

/// Builds a TransferAssetOp handing an asset this wallet currently owns to a
/// new owner's identity pubkey, signed with this wallet's identity key.
///
/// `required_kernel_excess_hex`, if provided, makes this the trustless
/// marketplace atomic-swap primitive: the transfer only becomes valid once a
/// transaction kernel with that exact excess exists on-chain (see
/// core::assets::TransferAssetOp::required_kernel_excess and
/// tx_kernel_excess_hex below). This lets a seller sign a transfer before a
/// buyer's payment lands, safely - it's cryptographically inert until that
/// payment is actually on-chain.
#[uniffi::export]
pub fn build_transfer_asset_request(keystore_bytes: Vec<u8>, asset_id: String, new_owner_pubkey_hex: String, required_kernel_excess_hex: Option<String>, required_royalty_kernel_excess_hex: Option<String>) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let new_owner_pubkey = Commitment::from_hex(&new_owner_pubkey_hex).ok_or_else(|| FfiError::InvalidHex(new_owner_pubkey_hex.clone()))?;
    let required_kernel_excess = match required_kernel_excess_hex {
        Some(hex) => Some(Commitment::from_hex(&hex).ok_or_else(|| FfiError::InvalidHex(hex.clone()))?),
        None => None,
    };
    let required_royalty_kernel_excess = match required_royalty_kernel_excess_hex {
        Some(hex) => Some(Commitment::from_hex(&hex).ok_or_else(|| FfiError::InvalidHex(hex.clone()))?),
        None => None,
    };

    let current_owner_secret = keystore.identity_key();
    let signature = TransferAssetOp::sign(&asset_id, &new_owner_pubkey, &required_kernel_excess, &required_royalty_kernel_excess, &current_owner_secret);

    let op = TransferAssetOp { asset_id, new_owner_pubkey, required_kernel_excess, required_royalty_kernel_excess, signature };
    serde_json::to_string(&op).map_err(|_| FfiError::TransferSerializationFailed)
}

/// Extracts a finalized (but not necessarily yet broadcast) transaction's
/// kernel excess as hex - used by a marketplace buyer to learn the exact
/// value to send the seller in a "want_transfer" inbox message, so the
/// seller can build a TransferAssetOp conditioned on this specific payment.
#[uniffi::export]
pub fn tx_kernel_excess_hex(transaction_json: String) -> Result<String, FfiError> {
    let tx: Transaction = serde_json::from_str(&transaction_json).map_err(|_| FfiError::InvalidTransactionJson)?;
    let kernel = tx.kernels.first().ok_or(FfiError::TransactionHasNoKernels)?;
    Ok(kernel.excess.to_hex())
}

// ---------- marketplace listings ----------

/// Builds a signed marketplace Listing advertising an asset this wallet
/// owns for sale at `price`, signed with this wallet's identity key.
#[uniffi::export]
pub fn build_create_listing_request(keystore_bytes: Vec<u8>, asset_id: String, price: u64, listed_at: u64) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let seller_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let seller_pubkey = Commitment(seller_secret * gens.B_blinding);

    let signature = Listing::sign(&asset_id, &seller_pubkey, price, listed_at, &seller_secret);
    let listing = Listing { asset_id, seller_pubkey, price, listed_at, signature };
    serde_json::to_string(&listing).map_err(|_| FfiError::ListingSerializationFailed)
}

/// Builds a signed cancellation for a listing this wallet previously
/// created - see POST /v1/marketplace/cancel.
#[uniffi::export]
pub fn build_cancel_listing_request(keystore_bytes: Vec<u8>, asset_id: String) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let seller_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let seller_pubkey = Commitment(seller_secret * gens.B_blinding);

    let signature = Signature::sign(&cancel_signing_message(&asset_id, &seller_pubkey), &seller_secret);

    #[derive(serde::Serialize)]
    struct CancelListingRequest {
        asset_id: String,
        seller_pubkey: Commitment,
        signature: Signature,
    }
    serde_json::to_string(&CancelListingRequest { asset_id, seller_pubkey, signature }).map_err(|_| FfiError::CancellationSerializationFailed)
}

// ---------- scheduled multi-phase collection launches ----------

/// Builds a signed LaunchCollectionOp for a scheduled multi-phase NFT drop -
/// `phases_json` is the JSON serialization of a `Vec<MintPhase>` (each phase
/// has `name`, `start_time`, `end_time`, `price`, `per_wallet_limit`, and an
/// optional `allowlist_merkle_root` - for an allowlisted phase, compute the
/// root client-side first via `compute_allowlist_merkle_proof`'s root_hex).
/// No fee_payment - launching costs nothing beyond ordinary block-inclusion.
#[uniffi::export]
pub fn build_launch_collection_request(keystore_bytes: Vec<u8>, collection_id: String, name: String, symbol: String, metadata: String, phases_json: String, royalty_bps: u16) -> Result<String, FfiError> {
    if royalty_bps > haze_chain::collections::MAX_ROYALTY_BPS {
        return Err(FfiError::RoyaltyTooHigh(haze_chain::collections::MAX_ROYALTY_BPS));
    }
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let creator_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let creator_pubkey = Commitment(creator_secret * gens.B_blinding);

    let phases: Vec<MintPhase> = serde_json::from_str(&phases_json).map_err(|e| FfiError::InvalidPhasesJson(e.to_string()))?;
    let metadata_bytes = metadata.into_bytes();

    let signature = LaunchCollectionOp::sign(&collection_id, &creator_pubkey, &name, &symbol, &metadata_bytes, &phases, royalty_bps, &creator_secret);
    let op = LaunchCollectionOp { collection_id, creator_pubkey, name, symbol, metadata: metadata_bytes, phases, royalty_bps, signature };
    serde_json::to_string(&op).map_err(|_| FfiError::CollectionLaunchSerializationFailed)
}

/// Sibling to build_mint_asset_request for a mint claimed against a
/// collection's scheduled phase - takes the same spendable-UTXO-funded
/// fee_payment path, plus the collection-drop fields.
/// `allowlist_proof_hex`/`allowlist_leaf_index` are only required when the
/// target phase actually has an allowlist_merkle_root;
/// `required_kernel_excess_hex`, if provided, is the payment-conditioning
/// primitive (same as build_transfer_asset_request's) - typically supplied
/// by the collection creator's auto-responding wallet, not built by the
/// minter themselves.
#[uniffi::export]
pub fn build_collection_mint_asset_request(
    keystore_bytes: Vec<u8>, store_bytes: Vec<u8>, asset_id: String, metadata: String, fee: u64,
    collection_id: String, phase_index: u32,
    allowlist_proof_hex: Option<Vec<String>>, allowlist_leaf_index: Option<u32>,
    required_kernel_excess_hex: Option<String>,
) -> Result<FfiMintAssetResult, FfiError> {
    validate_asset_id(&asset_id).map_err(|e| FfiError::InvalidAssetId(format!("{:?}", e)))?;
    if fee < ASSET_MINT_FEE {
        return Err(FfiError::FeeBelowAssetMintFloor(ASSET_MINT_FEE));
    }
    if metadata.len() > haze_chain::assets::MAX_METADATA_BYTES {
        return Err(FfiError::MetadataTooLarge(haze_chain::assets::MAX_METADATA_BYTES as u64));
    }

    let mut keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let store = WalletStore::from_bytes(&store_bytes).ok_or(FfiError::InvalidStore)?;

    let selected = planner::select_spendable_confirmed_only(&store, fee)
        .map_err(|e| match e {
            PlanError::InsufficientBalance { have, need } => FfiError::InsufficientBalance { have, need },
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

    let change_value = selected_total - fee;
    let (outputs, change, change_blinding) = if change_value > 0 {
        let change_index = keystore.allocate_index();
        let change_blinding = keystore.derive_blinding(change_index);
        let change_commitment = Commitment::new(change_value, change_blinding);
        let change_proof = RangeProof::prove(change_value, &change_blinding);
        let change_note = haze_crypto::note::seal(&keystore.note_key(), change_index, change_value);
        let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };
        let change_info = FfiOwnedOutput { index: change_index, value: change_value, commitment_hex: change_commitment.to_hex() };
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
            fee,
            signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
        }],
    };

    let metadata_bytes = metadata.into_bytes();
    let owner_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let owner_pubkey = Commitment(owner_secret * gens.B_blinding);

    let allowlist_proof = allowlist_proof_hex
        .map(|hexes| hexes.iter().map(|h| hex_to_32(h)).collect::<Result<Vec<[u8; 32]>, FfiError>>())
        .transpose()?;
    let required_kernel_excess = required_kernel_excess_hex
        .map(|h| Commitment::from_hex(&h).ok_or_else(|| FfiError::InvalidHex(h.clone())))
        .transpose()?;

    let collection_id_opt = Some(collection_id);
    let phase_index_opt = Some(phase_index);
    let signature = MintAssetOp::sign(&asset_id, &metadata_bytes, &collection_id_opt, &phase_index_opt, &required_kernel_excess, &owner_secret);

    let op = MintAssetOp {
        asset_id,
        owner_pubkey,
        metadata: metadata_bytes,
        fee_payment,
        collection_id: collection_id_opt,
        phase_index: phase_index_opt,
        allowlist_proof,
        allowlist_leaf_index,
        required_kernel_excess,
        signature,
        // Left unset here - see attach_creator_signature_to_mint below.
        // Building this op selects/spends this wallet's UTXOs for
        // fee_payment; that selection can't be redone once the creator's
        // approval comes back without risking a different (conflicting)
        // set of inputs, so the creator's signature is patched into this
        // exact already-built op_json afterward instead of rebuilding it.
        creator_signature: None,
    };
    let op_json = serde_json::to_string(&op).map_err(|_| FfiError::MintSerializationFailed)?;

    Ok(FfiMintAssetResult {
        op_json,
        updated_keystore_bytes: keystore.to_bytes(),
        spent_commitments_hex,
        change,
    })
}

/// Patches a collection creator's approval signature into an already-built
/// mint op_json (from build_collection_mint_asset_request), without needing
/// to rebuild it - rebuilding would re-select this wallet's spendable UTXOs
/// and could pick a different (conflicting) fee_payment.
#[uniffi::export]
pub fn attach_creator_signature_to_mint(op_json: String, creator_signature_hex: String) -> Result<String, FfiError> {
    let mut op: MintAssetOp = serde_json::from_str(&op_json).map_err(|_| FfiError::InvalidMintOpJson)?;
    op.creator_signature = Some(Signature::from_hex(&creator_signature_hex).ok_or_else(|| FfiError::InvalidHex(creator_signature_hex.clone()))?);
    serde_json::to_string(&op).map_err(|_| FfiError::MintSerializationFailed)
}

/// The collection creator's side of the approval handshake - signs approval
/// for one specific (asset_id, collection_id, phase_index,
/// required_kernel_excess, owner_pubkey) combination. The creator's own
/// wallet should independently verify (against the phase's
/// timing/allowlist/price and the actual on-chain payment) before calling
/// this - this function only produces the signature, it doesn't validate
/// anything itself.
#[uniffi::export]
pub fn sign_collection_mint_approval(keystore_bytes: Vec<u8>, asset_id: String, collection_id: String, phase_index: u32, required_kernel_excess_hex: String, owner_pubkey_hex: String) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let creator_secret = keystore.identity_key();
    let required_kernel_excess = Commitment::from_hex(&required_kernel_excess_hex).ok_or_else(|| FfiError::InvalidHex(required_kernel_excess_hex.clone()))?;
    let owner_pubkey = Commitment::from_hex(&owner_pubkey_hex).ok_or_else(|| FfiError::InvalidHex(owner_pubkey_hex.clone()))?;
    let signature = MintAssetOp::sign_collection_approval(&asset_id, &collection_id, phase_index, &required_kernel_excess, &owner_pubkey, &creator_secret);
    Ok(signature.to_hex())
}

fn hex_to_32(hex: &str) -> Result<[u8; 32], FfiError> {
    if hex.len() != 64 {
        return Err(FfiError::InvalidHex(hex.to_string()));
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| FfiError::InvalidHex(hex.to_string()))?;
    }
    Ok(bytes)
}

#[derive(uniffi::Record)]
pub struct FfiMerkleProofResult {
    pub proof_hex: Vec<String>,
    pub leaf_index: u32,
    pub root_hex: String,
}

/// Computes `target_pubkey_hex`'s Merkle inclusion proof against the full
/// plaintext allowlist `pubkeys_hex` (fetched from the off-chain allowlist
/// endpoint) - lets a minter get everything build_collection_mint_asset_request
/// needs without re-deriving the tree by hand. Returns an error if
/// target_pubkey_hex isn't actually present in the list.
#[uniffi::export]
pub fn compute_allowlist_merkle_proof(pubkeys_hex: Vec<String>, target_pubkey_hex: String) -> Result<FfiMerkleProofResult, FfiError> {
    let pubkeys: Vec<Commitment> = pubkeys_hex.iter()
        .map(|h| Commitment::from_hex(h).ok_or_else(|| FfiError::InvalidHex(h.clone())))
        .collect::<Result<Vec<_>, _>>()?;
    let target = Commitment::from_hex(&target_pubkey_hex).ok_or_else(|| FfiError::InvalidHex(target_pubkey_hex.clone()))?;

    let leaf_index = pubkeys.iter().position(|p| *p == target).ok_or(FfiError::TargetNotInAllowlist)?;

    let leaves: Vec<[u8; 32]> = pubkeys.iter().map(allowlist_leaf).collect();
    let root = merkle_root(&leaves);
    let proof = build_merkle_proof(&leaves, leaf_index);

    Ok(FfiMerkleProofResult {
        proof_hex: proof.iter().map(|p| p.iter().map(|b| format!("{:02x}", b)).collect()).collect(),
        leaf_index: leaf_index as u32,
        root_hex: root.iter().map(|b| format!("{:02x}", b)).collect(),
    })
}

/// Signs an allowlist publish so the off-chain, best-effort allowlist gossip
/// can be cross-checked against this collection's registered creator_pubkey
/// server-side. `pubkeys_hex` is the full plaintext list being published
/// for this collection/phase.
#[uniffi::export]
pub fn sign_allowlist_publish(keystore_bytes: Vec<u8>, collection_id: String, phase_index: u32, pubkeys_hex: Vec<String>, published_at: u64) -> Result<String, FfiError> {
    let keystore = Keystore::from_bytes(&keystore_bytes).ok_or(FfiError::InvalidKeystore)?;
    let creator_secret = keystore.identity_key();
    let gens = bulletproofs::PedersenGens::default();
    let creator_pubkey = Commitment(creator_secret * gens.B_blinding);

    let pubkeys: Vec<Commitment> = pubkeys_hex.iter()
        .map(|h| Commitment::from_hex(h).ok_or_else(|| FfiError::InvalidHex(h.clone())))
        .collect::<Result<Vec<_>, _>>()?;

    let signature = haze_chain::allowlist::AllowlistEntry::sign(&collection_id, phase_index, &pubkeys, published_at, &creator_secret);
    let entry = haze_chain::allowlist::AllowlistEntry { collection_id, phase_index, creator_pubkey, pubkeys, published_at, signature };
    serde_json::to_string(&entry).map_err(|_| FfiError::AllowlistSerializationFailed)
}
