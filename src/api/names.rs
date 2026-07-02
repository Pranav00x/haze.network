//! HTTP surface for the Haze Naming Registry (see core::registry). Registration
//! goes through the mempool + P2P gossip like a normal operation, since it's
//! only actually committed once a block including it is applied - these
//! handlers just accept/queue/broadcast, they don't mutate ChainState directly.
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use crate::core::chain::ChainState;
use crate::core::mempool::Mempool;
use crate::core::registry::{RegisterNameOp, NameRecord};
use crate::p2p::server::P2pServer;
use crate::p2p::message::P2pMessage;

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
