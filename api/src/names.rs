//! HTTP surface for the Haze Naming Registry (see core::registry). Registration
//! goes through the mempool + P2P gossip like a normal operation, since it's
//! only actually committed once a block including it is applied - these
//! handlers just accept/queue/broadcast, they don't mutate ChainState directly.
use std::sync::{Arc, Mutex};
use haze_chain::sync::LockExt;
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use haze_chain::chain::ChainState;
use haze_chain::mempool::Mempool;
use haze_chain::registry::{RegisterNameOp, TransferNameOp, NameRecord, validate_name};
use haze_crypto::pedersen::Commitment;
use haze_crypto::schnorr::Signature;
use haze_p2p::server::P2pServer;
use haze_p2p::message::P2pMessage;
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
        let c = chain.lock_recover();
        c.name_registry.contains_key(&op.name)
    };
    if already_taken {
        return Ok(error_reply(StatusCode::CONFLICT, format!("name '{}' is already registered", op.name)));
    }

    let added = {
        let mut mp = mempool.lock_recover();
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
        let c = chain.lock_recover();
        c.name_registry.contains_key(&req.name)
    };
    if already_taken {
        return Ok(error_reply(StatusCode::CONFLICT, format!("name '{}' is already registered", req.name)));
    }

    {
        let c = chain.lock_recover();
        faucet.reconcile(&c);
    }

    // Pays Mempool::suggested_name_fee rather than hardcoding
    // NAME_REGISTRATION_FEE here, so a change to the underlying constant
    // doesn't need a matching update at every call site.
    let suggested_fee = { mempool.lock_recover().suggested_name_fee() };
    let (fee_payment, spent, change_index) = match faucet.build_sponsored_fee_payment(suggested_fee) {
        Ok(built) => built,
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
        let mut mp = mempool.lock_recover();
        mp.add_name_op(op.clone())
    };
    if !added {
        // The fee payment was already built (and its inputs/change
        // optimistically marked Spent/Pending in the faucet's store) before
        // we knew the op itself couldn't be queued - undo that, or the
        // faucet's balance view permanently loses this amount even though
        // nothing was ever actually spent on-chain.
        faucet.revert_fee_payment(&spent, change_index);
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
        let c = chain.lock_recover();
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
        let mut mp = mempool.lock_recover();
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
        let c = chain.lock_recover();
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
    /// Filters to names owned by this identity pubkey (hex) - lets a wallet
    /// restored from only its BIP39 phrase rediscover which names it
    /// registered. Restore-from-phrase recovers funds (every output carries
    /// a recoverable note - see wallet::note) but has no way to recover
    /// *which* names it registered, since a NameRecord is a public registry
    /// entry keyed by the name string, not something derived from the seed
    /// - this filter is the other half: the identity_key itself IS
    /// recoverable from the phrase, so querying by it closes the gap.
    #[serde(default)]
    pub owner: Option<String>,
}

pub async fn handle_list_names(
    query: NamesListQuery,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let limit = query.limit.unwrap_or(50).min(500);
    let owner_filter = match &query.owner {
        Some(hex) => match Commitment::from_hex(hex) {
            Some(c) => Some(c),
            None => return Ok(error_reply(StatusCode::BAD_REQUEST, "invalid owner pubkey hex")),
        },
        None => None,
    };
    let mut records: Vec<NameRecord> = {
        let c = chain.lock_recover();
        c.name_registry.values()
            .filter(|r| owner_filter.map(|o| r.owner_pubkey == o).unwrap_or(true))
            .cloned().collect()
    };
    records.sort_by(|a, b| b.registered_at_block.cmp(&a.registered_at_block).then(a.name.cmp(&b.name)));
    records.truncate(limit);
    Ok(Box::new(warp::reply::json(&records)))
}
