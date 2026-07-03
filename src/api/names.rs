//! HTTP surface for the Haze Naming Registry (see core::registry). Registration
//! goes through the mempool + P2P gossip like a normal operation, since it's
//! only actually committed once a block including it is applied - these
//! handlers just accept/queue/broadcast, they don't mutate ChainState directly.
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use crate::core::chain::ChainState;
use crate::core::mempool::Mempool;
use crate::core::registry::{RegisterNameOp, TransferNameOp, NameRecord, validate_name};
use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use crate::p2p::server::P2pServer;
use crate::p2p::message::P2pMessage;
use super::faucet::FaucetState;

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn error_reply(status: StatusCode, message: impl Into<String>) -> Box<dyn warp::Reply> {
    Box::new(warp::reply::with_status(warp::reply::json(&ErrorResponse { error: message.into() }), status))
}

#[derive(Serialize)]
struct RegisterResponse {
    status: String,
}

pub async fn handle_register_name(
    op: RegisterNameOp,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if let Err(e) = op.validate_standalone() {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("invalid registration: {:?}", e)));
    }

    // Reject already-taken names immediately, rather than accepting a
    // "queued" response for something the proposer will silently drop at
    // block-assembly time and that will never actually get mined.
    let already_taken = {
        let c = chain.lock().unwrap();
        c.name_registry.contains_key(&op.name)
    };
    if already_taken {
        return Ok(error_reply(StatusCode::CONFLICT, format!("name '{}' is already registered", op.name)));
    }

    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_name_op(op.clone())
    };

    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "name already pending registration or has an unresolvable fee-payment input"));
    }

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewNameOp(op)).await;
    });

    Ok(Box::new(warp::reply::json(&RegisterResponse { status: "queued".to_string() })))
}

#[derive(Deserialize)]
pub struct SponsoredRegisterRequest {
    name: String,
    owner_pubkey: Commitment,
    resolves_to: Commitment,
    /// Signed by owner_pubkey's secret key - costs nothing to produce, since
    /// signing needs no funds. The node's own faucet identity covers the
    /// flat registration fee instead of the requester, so a wallet with a
    /// zero balance can still get a name (see FaucetState::build_sponsored_fee_payment).
    signature: Signature,
}

/// Same registration as handle_register_name, except the *fee* is paid by
/// this node's own faucet reserve instead of the requester - so a brand new
/// wallet with no funds at all can still register a name. Everything else
/// (name rules, ownership signature, on-chain validation once mined) is
/// identical; the resulting RegisterNameOp is indistinguishable on-chain
/// from a self-funded one.
pub async fn handle_sponsored_register_name(
    req: SponsoredRegisterRequest,
    faucet: Arc<FaucetState>,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if let Err(e) = validate_name(&req.name) {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("invalid name: {:?}", e)));
    }

    let msg = RegisterNameOp::signing_message(&req.name);
    if !req.signature.verify(&msg, &req.owner_pubkey) {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "signature does not match owner_pubkey"));
    }

    let already_taken = {
        let c = chain.lock().unwrap();
        c.name_registry.contains_key(&req.name)
    };
    if already_taken {
        return Ok(error_reply(StatusCode::CONFLICT, format!("name '{}' is already registered", req.name)));
    }

    {
        let c = chain.lock().unwrap();
        faucet.reconcile(&c);
    }

    // Pays the live congestion-priced suggestion (see Mempool::
    // suggested_name_fee), not the bare NAME_REGISTRATION_FEE floor - a
    // sponsored registration should queue-jump the same as a self-funded one
    // would if it chose to pay more, rather than always sitting at the back
    // of a busy backlog.
    let suggested_fee = { mempool.lock().unwrap().suggested_name_fee() };
    let fee_payment = match faucet.build_sponsored_fee_payment(suggested_fee) {
        Ok(tx) => tx,
        Err(_) => return Ok(error_reply(StatusCode::SERVICE_UNAVAILABLE, "sponsor reserve temporarily depleted - try again shortly")),
    };

    let op = RegisterNameOp {
        name: req.name,
        owner_pubkey: req.owner_pubkey,
        resolves_to: req.resolves_to,
        fee_payment,
        signature: req.signature,
    };

    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_name_op(op.clone())
    };
    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "name already pending registration"));
    }

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewNameOp(op)).await;
    });

    Ok(Box::new(warp::reply::json(&RegisterResponse { status: "queued".to_string() })))
}

pub async fn handle_transfer_name(
    op: TransferNameOp,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let current = {
        let c = chain.lock().unwrap();
        c.name_registry.get(&op.name).cloned()
    };
    let Some(current) = current else {
        return Ok(error_reply(StatusCode::NOT_FOUND, format!("name '{}' is not registered", op.name)));
    };

    let msg = TransferNameOp::signing_message(&op.name, &op.new_owner_pubkey, &op.new_resolves_to);
    if !op.signature.verify(&msg, &current.owner_pubkey) {
        return Ok(error_reply(StatusCode::FORBIDDEN, "signature does not match the name's current owner"));
    }

    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_transfer_op(op.clone())
    };
    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "name already has a pending transfer"));
    }

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewTransferOp(op)).await;
    });

    Ok(Box::new(warp::reply::json(&RegisterResponse { status: "queued".to_string() })))
}

pub async fn handle_resolve_name(
    name: String,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let record = {
        let c = chain.lock().unwrap();
        c.name_registry.get(&name).cloned()
    };
    match record {
        Some(r) => Ok(Box::new(warp::reply::json(&r))),
        None => Ok(error_reply(StatusCode::NOT_FOUND, format!("name '{}' is not registered", name))),
    }
}

#[derive(Deserialize)]
pub struct NamesListQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

pub async fn handle_list_names(
    query: NamesListQuery,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let limit = query.limit.unwrap_or(50).min(500);
    let mut records: Vec<NameRecord> = {
        let c = chain.lock().unwrap();
        c.name_registry.values().cloned().collect()
    };
    records.sort_by(|a, b| b.registered_at_block.cmp(&a.registered_at_block).then(a.name.cmp(&b.name)));
    records.truncate(limit);
    Ok(Box::new(warp::reply::json(&records)))
}
