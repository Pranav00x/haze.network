//! HTTP surface for off-chain collection-phase allowlists (see
//! core::allowlist). Only a phase's Merkle root is consensus (see
//! core::collections::MintPhase) - the full plaintext list lives here so any
//! wallet/marketplace client can fetch it and compute its own inclusion
//! proof client-side. Mirrors api::marketplace.rs's handler shapes.
use std::sync::{Arc, Mutex};
use serde::Serialize;
use warp::http::StatusCode;

use haze_chain::chain::ChainState;
use haze_chain::allowlist::{AllowlistEntry, AllowlistState};
use haze_p2p::server::P2pServer;
use haze_p2p::message::P2pMessage;

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn error_reply(status: StatusCode, message: impl Into<String>) -> Box<dyn warp::Reply> {
    Box::new(warp::reply::with_status(warp::reply::json(&ErrorResponse { error: message.into() }), status))
}

#[derive(Serialize)]
struct QueuedResponse {
    status: String,
}

pub async fn handle_publish_allowlist(
    entry: AllowlistEntry,
    allowlist: Arc<AllowlistState>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if let Err(e) = entry.validate_standalone() {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("invalid allowlist publish signature: {:?}", e)));
    }

    let creator_ok = {
        let c = chain.lock().unwrap();
        entry.validate_against_registry(&c.collection_registry).is_ok()
    };
    if !creator_ok {
        return Ok(error_reply(StatusCode::FORBIDDEN, "signer is not this collection's registered creator"));
    }

    allowlist.publish(entry.clone());

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewAllowlist(entry)).await;
    });

    Ok(Box::new(warp::reply::json(&QueuedResponse { status: "published".to_string() })))
}

pub async fn handle_get_allowlist(
    collection_id: String,
    phase_index: u32,
    allowlist: Arc<AllowlistState>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    match allowlist.get(&collection_id, phase_index) {
        Some(entry) => Ok(Box::new(warp::reply::json(&entry))),
        None => Ok(error_reply(StatusCode::NOT_FOUND, format!("no allowlist published for collection '{}' phase {}", collection_id, phase_index))),
    }
}
