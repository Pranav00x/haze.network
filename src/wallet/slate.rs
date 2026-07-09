//! Two-party ("slate") payment protocol: lets two different wallets build a
//! single valid Mimblewimble transaction together, each contributing their own
//! secret blinding factor, without either side ever learning the other's
//! secret. Mimblewimble has no addresses, so the two sides exchange a plain
//! JSON `Slate` file out-of-band (email, chat, etc.) - transport is
//! deliberately left to the user, this module only implements the crypto
//! protocol itself.
//!
//! Flow: sender calls `create_slate` (produces a `Slate` to hand to the
//! recipient, plus a `PendingSlate` it keeps locally and never shares) ->
//! recipient calls `respond_to_slate` (fills in the slate, hands it back) ->
//! sender calls `finalize_slate` (combines both sides' contributions into the
//! final `Transaction` to validate and broadcast).

use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::Path;
use serde::{Serialize, Deserialize};
use curve25519_dalek_ng::scalar::Scalar;

use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::{self, Signature};

const WALLET_DIR: &str = "wallet_data";
const PENDING_SLATE_FILE: &str = "wallet_data/pending_slate.dat";
use crate::core::mempool::required_fee;
use crate::core::transaction::{Transaction, Input, Output, TxKernel};
use super::keystore::Keystore;
use super::note;
use super::store::WalletStore;
use super::planner::{blinding_for, select_spendable, PlanError};

/// Same reasoning as planner::plan_send's MAX_FEE_FIT_ATTEMPTS.
const MAX_FEE_FIT_ATTEMPTS: u32 = 16;

/// NOTE: this can't be widened beyond `fee` alone (e.g. to also bind the
/// sender's inputs/change) without also changing
/// core::transaction::Transaction::validate_with_reward's kernel-signature
/// message, which is hardcoded to exactly `fee.to_le_bytes()` and checked
/// by every node on every kernel (coinbase, genesis, fee payments, and
/// this two-party flow alike) - that's a consensus-wide, hard-fork-scale
/// change, not something to do for this flow alone. A prior attempt at
/// this hardening broke `tx.validate()` for exactly this reason (see git
/// history) and was reverted.
fn slate_challenge_message(fee: u64) -> Vec<u8> {
    fee.to_le_bytes().to_vec()
}

/// The file two parties exchange. Only ever carries public data (commitments,
/// range proofs, public nonce/excess points) - secret blinding factors and
/// nonces never leave the side that generated them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Slate {
    pub amount: u64,
    pub fee: u64,
    pub inputs: Vec<Input>,
    pub sender_output: Option<Output>,
    pub sender_excess_point: Commitment,
    pub sender_nonce_point: Commitment,
    pub receiver_output: Option<Output>,
    pub receiver_excess_point: Option<Commitment>,
    pub receiver_nonce_point: Option<Commitment>,
    pub receiver_partial_sig: Option<Scalar>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeInfo {
    pub index: u32,
    pub output: Output,
    pub value: u64,
}

/// The sender's private local state between create_slate and finalize_slate -
/// never shared with the recipient. Persisted to wallet_data/pending_slate.dat.
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingSlate {
    pub fee: u64,
    pub spent_commitments: Vec<Commitment>,
    pub change: Option<ChangeInfo>,
    pub excess_blinding: Scalar,
    pub nonce: Scalar,
    pub nonce_point: Commitment,
}

impl PendingSlate {
    /// Persists this pending slate, overwriting any previous one - only one
    /// in-flight outgoing slate is supported per wallet at a time.
    pub fn save(&self) {
        if !Path::new(WALLET_DIR).exists() {
            fs::create_dir(WALLET_DIR).unwrap();
        }
        let encoded = bincode::serialize(self).unwrap();
        let mut file = File::create(PENDING_SLATE_FILE).unwrap();
        file.write_all(&encoded).unwrap();
    }

    pub fn load() -> Option<Self> {
        if !Path::new(PENDING_SLATE_FILE).exists() {
            return None;
        }
        let mut file = File::open(PENDING_SLATE_FILE).ok()?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).ok()?;
        bincode::deserialize(&buffer).ok()
    }

    /// Removes the pending slate once it's been finalized and broadcast.
    pub fn delete() {
        let _ = fs::remove_file(PENDING_SLATE_FILE);
    }
}

pub struct OwnedOutputInfo {
    pub index: u32,
    pub value: u64,
    pub commitment: Commitment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlateError {
    IncompleteResponse,
}

/// Sender step 1: selects confirmed inputs covering amount+fee, builds an
/// optional change output, and derives its half of the kernel signature.
/// Allocates (and persists) any new output index immediately, same
/// crash-safety convention as plan_send.
///
/// `fee` is a starting guess auto-corrected to match the eventual finalized
/// transaction's real size (see planner::plan_send's doc comment for why -
/// same underlying problem here). Unlike plan_send, correcting this after
/// the recipient has already countersigned isn't an option - once agreed,
/// the fee is baked into both sides' partial signatures and can't be
/// silently bumped without restarting the whole interactive protocol. But
/// every output (Input/Output/TxKernel) serializes to the same fixed byte
/// size regardless of whose commitment it is or what value it hides, so the
/// finalized transaction's exact size - our own inputs/change plus exactly
/// one recipient output - is fully knowable before ever contacting the
/// recipient. This fits the fee against a same-shaped probe transaction
/// (a placeholder standing in for the not-yet-known recipient output) up
/// front, so the recipient only ever sees an already-correctly-priced slate.
pub fn create_slate(
    keystore: &mut Keystore,
    store: &WalletStore,
    amount: u64,
    fee: u64,
) -> Result<(Slate, PendingSlate), PlanError> {
    let mut fee = fee;
    for _ in 0..MAX_FEE_FIT_ATTEMPTS {
        let (built_slate, pending, probe_tx) = build_slate(keystore, store, amount, fee)?;
        let required = required_fee(&probe_tx);
        if fee >= required {
            return Ok((built_slate, pending));
        }
        fee = required;
    }
    let target = amount + fee;
    Err(PlanError::InsufficientBalance { have: store.balance() + store.pending_balance(), need: target })
}

fn build_slate(
    keystore: &mut Keystore,
    store: &WalletStore,
    amount: u64,
    fee: u64,
) -> Result<(Slate, PendingSlate, Transaction), PlanError> {
    let target = amount + fee;
    let selected = select_spendable(store, target)?;
    let selected_total: u64 = selected.iter().map(|(_, _, value)| value).sum();

    let mut input_blindings: Vec<Scalar> = Vec::new();
    let mut inputs: Vec<Input> = Vec::new();
    let mut spent_commitments: Vec<Commitment> = Vec::new();
    for (index, commitment, _value) in &selected {
        input_blindings.push(blinding_for(keystore, *index));
        inputs.push(Input { commitment: *commitment });
        spent_commitments.push(*commitment);
    }

    let change_value = selected_total - target;
    let (sender_output, change, change_blinding) = if change_value > 0 {
        let change_index = keystore.allocate_index();
        let change_blinding = keystore.derive_blinding(change_index);
        let change_commitment = Commitment::new(change_value, change_blinding);
        let change_proof = RangeProof::prove(change_value, &change_blinding);
        let change_note = note::seal(&keystore.note_key(), change_index, change_value);
        let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };
        (
            Some(output.clone()),
            Some(ChangeInfo { index: change_index, output, value: change_value }),
            change_blinding,
        )
    } else {
        (None, None, Scalar::zero())
    };

    let sum_input_blinding: Scalar = input_blindings.iter().sum();
    let excess_blinding = sum_input_blinding - change_blinding;
    let (nonce, nonce_point) = schnorr::generate_nonce();

    // A same-shaped stand-in for the finalized transaction, built before the
    // recipient is ever contacted (see this function's doc comment): our
    // real inputs and change (if any), plus one placeholder output sized
    // exactly like the recipient's eventual real one - every Output
    // serializes to the same byte length regardless of whose commitment or
    // value it carries, so this measures the real final size exactly.
    let mut probe_outputs: Vec<Output> = sender_output.iter().cloned().collect();
    let placeholder_note = note::seal(&keystore.note_key(), 0, 0);
    probe_outputs.push(Output {
        commitment: Commitment::new(amount, Scalar::zero()),
        proof: RangeProof::prove(amount, &Scalar::zero()),
        note: placeholder_note,
    });
    let probe_tx = Transaction {
        inputs: inputs.clone(),
        outputs: probe_outputs,
        kernels: vec![TxKernel { excess: Commitment::new(0, excess_blinding), fee, signature: Signature { s: Scalar::zero(), e: Scalar::zero() } }],
    };

    let slate = Slate {
        amount,
        fee,
        inputs,
        sender_output,
        sender_excess_point: Commitment::new(0, excess_blinding),
        sender_nonce_point: Commitment(nonce_point),
        receiver_output: None,
        receiver_excess_point: None,
        receiver_nonce_point: None,
        receiver_partial_sig: None,
    };

    let pending = PendingSlate {
        fee,
        spent_commitments,
        change,
        excess_blinding,
        nonce,
        nonce_point: Commitment(nonce_point),
    };

    Ok((slate, pending, probe_tx))
}

/// Receiver step: builds an output for `slate.amount`, derives its half of the
/// kernel signature against the aggregate (sender + receiver) point, and
/// returns the filled-in slate to hand back, plus the output info the caller
/// should persist into its own WalletStore as Pending. Allocates (and the
/// caller must persist) a new keystore index.
pub fn respond_to_slate(keystore: &mut Keystore, slate: &Slate) -> (Slate, OwnedOutputInfo) {
    let out_index = keystore.allocate_index();
    let r_out = keystore.derive_blinding(out_index);
    let out_commitment = Commitment::new(slate.amount, r_out);
    let out_proof = RangeProof::prove(slate.amount, &r_out);
    let out_note = note::seal(&keystore.note_key(), out_index, slate.amount);
    let output = Output { commitment: out_commitment, proof: out_proof, note: out_note };

    let k_r = Scalar::zero() - r_out;
    let receiver_excess_point = Commitment::new(0, k_r);
    let (nonce, nonce_point) = schnorr::generate_nonce();
    let receiver_nonce_point = Commitment(nonce_point);

    let agg_excess_point = slate.sender_excess_point.as_point() + receiver_excess_point.as_point();
    let agg_nonce_point = slate.sender_nonce_point.as_point() + receiver_nonce_point.as_point();

    let e = schnorr::compute_challenge(&slate_challenge_message(slate.fee), agg_excess_point, agg_nonce_point);
    let partial_sig = schnorr::partial_sign(&nonce, &k_r, &e);

    let mut response = slate.clone();
    response.receiver_output = Some(output);
    response.receiver_excess_point = Some(receiver_excess_point);
    response.receiver_nonce_point = Some(receiver_nonce_point);
    response.receiver_partial_sig = Some(partial_sig);

    let info = OwnedOutputInfo { index: out_index, value: slate.amount, commitment: out_commitment };
    (response, info)
}

/// Sender step 2 (final): recomputes the aggregate challenge, computes the
/// sender's own partial signature, combines it with the receiver's, and
/// assembles the final Transaction. Pure - the caller validates and
/// broadcasts it, and only on success applies `pending`'s spent_commitments/
/// change to its WalletStore.
pub fn finalize_slate(pending: &PendingSlate, response: &Slate) -> Result<Transaction, SlateError> {
    let receiver_output = response.receiver_output.clone().ok_or(SlateError::IncompleteResponse)?;
    let receiver_excess_point = response.receiver_excess_point.ok_or(SlateError::IncompleteResponse)?;
    let receiver_nonce_point = response.receiver_nonce_point.ok_or(SlateError::IncompleteResponse)?;
    let receiver_partial_sig = response.receiver_partial_sig.ok_or(SlateError::IncompleteResponse)?;

    let sender_excess_point = Commitment::new(0, pending.excess_blinding).as_point();
    let agg_excess_point = sender_excess_point + receiver_excess_point.as_point();
    let agg_nonce_point = pending.nonce_point.as_point() + receiver_nonce_point.as_point();

    let e = schnorr::compute_challenge(&slate_challenge_message(pending.fee), agg_excess_point, agg_nonce_point);
    let sender_partial_sig = schnorr::partial_sign(&pending.nonce, &pending.excess_blinding, &e);
    let signature = schnorr::aggregate(&[sender_partial_sig, receiver_partial_sig], e);

    let inputs: Vec<Input> = pending.spent_commitments.iter().map(|c| Input { commitment: *c }).collect();
    let mut outputs = Vec::new();
    if let Some(change) = &pending.change {
        outputs.push(change.output.clone());
    }
    outputs.push(receiver_output);

    let kernel = TxKernel {
        excess: Commitment(agg_excess_point),
        fee: pending.fee,
        signature,
    };

    Ok(Transaction { inputs, outputs, kernels: vec![kernel] })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::store::OutputStatus;

    fn fund(keystore: &mut Keystore, store: &mut WalletStore, value: u64) {
        let index = keystore.allocate_index();
        let blinding = keystore.derive_blinding(index);
        let commitment = Commitment::new(value, blinding);
        store.add_output(index, value, commitment, OutputStatus::Confirmed);
    }

    #[test]
    fn two_party_flow_with_change_produces_valid_transaction() {
        let mut sender_keystore = Keystore::generate();
        let mut sender_store = WalletStore::default();
        fund(&mut sender_keystore, &mut sender_store, 1000);

        let (slate, pending) = create_slate(&mut sender_keystore, &sender_store, 100, 5).unwrap();

        let mut receiver_keystore = Keystore::generate();
        let (response, receiver_info) = respond_to_slate(&mut receiver_keystore, &slate);
        assert_eq!(receiver_info.value, 100);

        let tx = finalize_slate(&pending, &response).unwrap();
        assert!(tx.validate(), "two-party transaction with change must validate");
        assert_eq!(tx.outputs.len(), 2); // sender's change + receiver's output
        assert_eq!(tx.inputs.len(), 1);
    }

    #[test]
    fn create_slate_auto_corrects_an_undersized_fee_guess_for_a_transaction_with_change() {
        let mut sender_keystore = Keystore::generate();
        let mut sender_store = WalletStore::default();
        fund(&mut sender_keystore, &mut sender_store, 1000);

        // 5 is only enough for the bare 1-input/1-output shape; this slate
        // produces sender change plus the recipient's output (2 outputs
        // total once finalized), which is bigger and so needs more - and
        // the pre-contact size probe should have already caught that before
        // the recipient ever saw the slate.
        let (slate, pending) = create_slate(&mut sender_keystore, &sender_store, 100, 5).unwrap();
        assert!(slate.fee > 5, "the probe should have corrected the fee before the recipient was ever contacted");

        let mut receiver_keystore = Keystore::generate();
        let (response, _receiver_info) = respond_to_slate(&mut receiver_keystore, &slate);
        let tx = finalize_slate(&pending, &response).unwrap();
        assert!(tx.validate());

        assert_eq!(tx.kernels[0].fee, required_fee(&tx), "the finalized transaction's fee should exactly match what its real size requires");

        let mut mempool = crate::core::mempool::Mempool::new();
        assert!(mempool.add_transaction(tx), "the auto-corrected finalized transaction must actually be accepted");
    }

    #[test]
    fn two_party_flow_exact_amount_has_no_change() {
        let mut sender_keystore = Keystore::generate();
        let mut sender_store = WalletStore::default();
        fund(&mut sender_keystore, &mut sender_store, 105);

        let (slate, pending) = create_slate(&mut sender_keystore, &sender_store, 100, 5).unwrap();

        let mut receiver_keystore = Keystore::generate();
        let (response, _receiver_info) = respond_to_slate(&mut receiver_keystore, &slate);

        let tx = finalize_slate(&pending, &response).unwrap();
        assert!(tx.validate(), "two-party transaction without change must validate");
        assert_eq!(tx.outputs.len(), 1);
    }

    #[test]
    fn two_party_flow_with_multiple_inputs_produces_valid_transaction() {
        let mut sender_keystore = Keystore::generate();
        let mut sender_store = WalletStore::default();
        fund(&mut sender_keystore, &mut sender_store, 60);
        fund(&mut sender_keystore, &mut sender_store, 60);

        let (slate, pending) = create_slate(&mut sender_keystore, &sender_store, 100, 5).unwrap();
        assert_eq!(slate.inputs.len(), 2);

        let mut receiver_keystore = Keystore::generate();
        let (response, _receiver_info) = respond_to_slate(&mut receiver_keystore, &slate);

        let tx = finalize_slate(&pending, &response).unwrap();
        assert!(tx.validate());
        assert_eq!(tx.inputs.len(), 2);
    }

    #[test]
    fn create_slate_rejects_insufficient_balance() {
        let mut keystore = Keystore::generate();
        let mut store = WalletStore::default();
        fund(&mut keystore, &mut store, 50);

        let err = create_slate(&mut keystore, &store, 100, 5).unwrap_err();
        assert_eq!(err, PlanError::InsufficientBalance { have: 50, need: 105 });
    }

    #[test]
    fn finalize_slate_rejects_incomplete_response() {
        let mut sender_keystore = Keystore::generate();
        let mut sender_store = WalletStore::default();
        fund(&mut sender_keystore, &mut sender_store, 1000);

        let (slate, pending) = create_slate(&mut sender_keystore, &sender_store, 100, 5).unwrap();

        let err = finalize_slate(&pending, &slate).unwrap_err();
        assert_eq!(err, SlateError::IncompleteResponse);
    }
}
