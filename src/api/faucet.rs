//! Repeatable devnet faucet, distinct from the wallet's single-use
//! claim-genesis convenience. Funded by a second well-known genesis output
//! (see core::genesis::FAUCET_RESERVE_BLINDING) that only this node's own
//! embedded wallet identity ever spends from - every request runs the same
//! two-party slate protocol (wallet::slate) the web wallet already uses for
//! peer-to-peer payments, just with this node playing the sender.
use std::sync::Mutex;
use std::collections::HashSet;
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use crate::core::chain::ChainState;
use crate::core::genesis::{FAUCET_RESERVE_BLINDING, FAUCET_RESERVE_VALUE};
use crate::core::mempool::Mempool;
use crate::crypto::pedersen::Commitment;
use crate::wallet::keystore::Keystore;
use crate::wallet::store::{WalletStore, OutputStatus, FAUCET_INDEX};
use crate::wallet::slate::{self, PendingSlate, Slate};
use crate::wallet::planner::PlanError;

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
        let commitment = Commitment::new(FAUCET_RESERVE_VALUE, curve25519_dalek_ng::scalar::Scalar::from(FAUCET_RESERVE_BLINDING));
        store.add_output(FAUCET_INDEX, FAUCET_RESERVE_VALUE, commitment, OutputStatus::Confirmed);
        Self {
            keystore: Mutex::new(keystore),
            store: Mutex::new(store),
            pending: Mutex::new(None),
        }
    }

    fn reconcile(&self, chain: &ChainState) {
        let utxos: HashSet<Commitment> = chain.utxos.iter().cloned().collect();
        self.store.lock().unwrap().reconcile(&utxos);
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

    match slate::create_slate(&mut keystore, &store, req.amount, 0) {
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
