//! Repeatable devnet faucet, distinct from the wallet's single-use
//! claim-genesis convenience. Funded by the treasury genesis allocation (see
//! core::genesis::TREASURY_BLINDING) that only this node's own embedded
//! wallet identity ever spends from - every request runs the same two-party
//! slate protocol (wallet::slate) the web wallet already uses for
//! peer-to-peer payments, just with this node playing the sender.
use std::sync::Mutex;
use std::collections::HashSet;
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use crate::core::chain::ChainState;
use crate::core::genesis::{TREASURY_BLINDING, TREASURY_ALLOCATION};
use crate::core::mempool::Mempool;
use crate::core::transaction::{Transaction, Input, Output, TxKernel};
use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use crate::wallet::keystore::Keystore;
use crate::wallet::store::{WalletStore, OutputStatus, FAUCET_INDEX};
use crate::wallet::slate::{self, PendingSlate, Slate};
use crate::wallet::planner::{self, PlanError};
use curve25519_dalek_ng::scalar::Scalar;

/// Devnet-only cap per request - keeps a single requester from draining the
/// reserve, not a real anti-abuse measure.
const MAX_FAUCET_AMOUNT: u64 = 1000;

pub struct FaucetState {
    keystore: Mutex<Keystore>,
    store: Mutex<WalletStore>,
    /// Only one faucet payout in flight at a time - simpler than juggling
    /// concurrent PendingSlates, and faucet requests aren't latency-sensitive.
    pending: Mutex<Option<PendingSlate>>,
}

impl FaucetState {
    pub fn new() -> Self {
        let keystore = Keystore::generate();
        let mut store = WalletStore::default();
        let commitment = Commitment::new(TREASURY_ALLOCATION, curve25519_dalek_ng::scalar::Scalar::from(TREASURY_BLINDING));
        store.add_output(FAUCET_INDEX, TREASURY_ALLOCATION, commitment, OutputStatus::Confirmed);
        Self {
            keystore: Mutex::new(keystore),
            store: Mutex::new(store),
            pending: Mutex::new(None),
        }
    }

    pub(crate) fn reconcile(&self, chain: &ChainState) {
        let utxos: HashSet<Commitment> = chain.utxos.iter().cloned().collect();
        self.store.lock().unwrap().reconcile(&utxos);
    }

    /// Builds a plain fee-paying transaction from the faucet's own reserve -
    /// no destination output, no two-party protocol needed, since this only
    /// ever sponsors someone ELSE's fee (see api/names.rs's sponsored
    /// registration), not a payment to them. This is what lets a brand-new
    /// wallet with zero balance still register a name: it signs the
    /// registration itself (free - just needs its own secret key), and the
    /// faucet covers the flat fee.
    pub fn build_sponsored_fee_payment(&self, fee: u64) -> Result<Transaction, PlanError> {
        let mut keystore = self.keystore.lock().unwrap();
        let mut store = self.store.lock().unwrap();

        let selected = planner::select_spendable(&store, fee)?;
        let selected_total: u64 = selected.iter().map(|(_, _, v)| v).sum();

        let mut input_blindings: Vec<Scalar> = Vec::new();
        let mut inputs: Vec<Input> = Vec::new();
        let mut spent: Vec<Commitment> = Vec::new();
        for (index, commitment, _value) in &selected {
            input_blindings.push(planner::blinding_for(&keystore, *index));
            inputs.push(Input { commitment: *commitment });
            spent.push(*commitment);
        }

        let change_value = selected_total - fee;
        let (outputs, change_blinding) = if change_value > 0 {
            let change_index = keystore.allocate_index();
            let change_blinding = keystore.derive_blinding(change_index);
            let change_commitment = Commitment::new(change_value, change_blinding);
            let change_proof = RangeProof::prove(change_value, &change_blinding);
            let change_note = crate::wallet::note::seal(&keystore.note_key(), change_index, change_value);
            let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };
            store.add_output(change_index, change_value, change_commitment, OutputStatus::Pending);
            (vec![output], change_blinding)
        } else {
            (vec![], Scalar::zero())
        };

        for c in &spent {
            store.mark_spent(c);
        }

        let sum_input_blinding: Scalar = input_blindings.iter().sum();
        let excess_r = sum_input_blinding - change_blinding;
        let kernel = TxKernel {
            excess: Commitment::new(0, excess_r),
            fee,
            signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
        };

        Ok(Transaction { inputs, outputs, kernels: vec![kernel] })
    }
}

#[derive(Deserialize)]
pub struct FaucetRequest {
    amount: u64,
}

#[derive(Serialize)]
struct FaucetSlateResponse {
    slate_json: String,
}

#[derive(Serialize)]
struct FaucetErrorResponse {
    error: String,
}

fn error_reply(status: StatusCode, message: impl Into<String>) -> Box<dyn warp::Reply> {
    Box::new(warp::reply::with_status(warp::reply::json(&FaucetErrorResponse { error: message.into() }), status))
}

pub async fn handle_faucet_request(
    req: FaucetRequest,
    faucet: std::sync::Arc<FaucetState>,
    chain: std::sync::Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if req.amount == 0 || req.amount > MAX_FAUCET_AMOUNT {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("amount must be between 1 and {}", MAX_FAUCET_AMOUNT)));
    }

    {
        let c = chain.lock().unwrap();
        faucet.reconcile(&c);
    }

    let mut pending_guard = faucet.pending.lock().unwrap();
    if pending_guard.is_some() {
        return Ok(error_reply(StatusCode::CONFLICT, "faucet is completing another request, try again in a few seconds"));
    }

    let mut keystore = faucet.keystore.lock().unwrap();
    let store = faucet.store.lock().unwrap();

    // Pays the mempool's fee floor (see core::mempool::MIN_FEE) from the
    // faucet's own reserve, on top of req.amount - the requester still gets
    // the full amount they asked for, since plan_send/create_slate's fee is
    // additional to the destination output, not deducted from it. A flat 0
    // used to work fine here since nothing enforced a minimum; now that
    // add_transaction rejects anything under MIN_FEE, this transaction needs
    // a real fee to even enter a mempool.
    match slate::create_slate(&mut keystore, &store, req.amount, crate::core::mempool::MIN_FEE) {
        Ok((built_slate, pending)) => {
            *pending_guard = Some(pending);
            let slate_json = serde_json::to_string(&built_slate).unwrap();
            Ok(Box::new(warp::reply::json(&FaucetSlateResponse { slate_json })))
        }
        Err(PlanError::InsufficientBalance { .. }) => {
            Ok(error_reply(StatusCode::SERVICE_UNAVAILABLE, "faucet reserve temporarily depleted (recent payouts still confirming) - try again shortly"))
        }
    }
}

#[derive(Deserialize)]
pub struct FaucetCompleteRequest {
    response_slate_json: String,
}

#[derive(Serialize)]
struct FaucetCompleteResponse {
    status: String,
}

pub async fn handle_faucet_complete(
    req: FaucetCompleteRequest,
    faucet: std::sync::Arc<FaucetState>,
    mempool: std::sync::Arc<Mutex<Mempool>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let pending = {
        let mut pending_guard = faucet.pending.lock().unwrap();
        match pending_guard.take() {
            Some(p) => p,
            None => return Ok(error_reply(StatusCode::BAD_REQUEST, "no pending faucet request - call /v1/faucet first")),
        }
    };

    let response: Slate = match serde_json::from_str(&req.response_slate_json) {
        Ok(s) => s,
        Err(_) => return Ok(error_reply(StatusCode::BAD_REQUEST, "invalid response slate JSON")),
    };

    let transaction = match slate::finalize_slate(&pending, &response) {
        Ok(tx) => tx,
        Err(_) => return Ok(error_reply(StatusCode::BAD_REQUEST, "incomplete response slate")),
    };

    if !transaction.validate() {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "constructed faucet transaction failed validation"));
    }

    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_transaction(transaction)
    };

    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "mempool rejected the faucet transaction"));
    }

    // Applied optimistically (before mining), same convention as the web
    // wallet's own commit_send/commit_slate_send - avoids the reserve output
    // getting re-selected by a second request before this one confirms.
    let mut store = faucet.store.lock().unwrap();
    for commitment in &pending.spent_commitments {
        store.mark_spent(commitment);
    }
    if let Some(change) = &pending.change {
        store.add_output(change.index, change.value, change.output.commitment, OutputStatus::Pending);
    }

    Ok(Box::new(warp::reply::json(&FaucetCompleteResponse { status: "success".to_string() })))
}
