//! HTTP surface for collection launches / scheduled multi-phase minting
//! (see core::collections). Launches go through the mempool + P2P gossip
//! like any other consensus-bound op - these handlers just accept/queue/
//! broadcast, they don't mutate ChainState directly. Mirrors api::assets.rs's
//! handler shapes.
use std::sync::{Arc, Mutex};
use serde::Deserialize;
use warp::http::StatusCode;

use crate::core::chain::ChainState;
use crate::core::mempool::Mempool;
use crate::core::collections::{LaunchCollectionOp, CollectionRecord};
use crate::p2p::server::P2pServer;
use crate::p2p::message::P2pMessage;

#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
}

fn error_reply(status: StatusCode, message: impl Into<String>) -> Box<dyn warp::Reply> {
    Box::new(warp::reply::with_status(warp::reply::json(&ErrorResponse { error: message.into() }), status))
}

#[derive(serde::Serialize)]
struct QueuedResponse {
    status: String,
}

pub async fn handle_launch_collection(
    op: LaunchCollectionOp,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if let Err(e) = op.validate_standalone() {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("invalid collection launch: {:?}", e)));
    }

    // Reject already-launched collection_ids immediately, rather than
    // accepting a "queued" response for something the proposer will
    // silently drop at block-assembly time (mirrors handle_mint_asset's
    // early asset_id-uniqueness check).
    let already_launched = {
        let c = chain.lock().unwrap();
        c.collection_registry.contains_key(&op.collection_id)
    };
    if already_launched {
        return Ok(error_reply(StatusCode::CONFLICT, format!("collection '{}' is already launched", op.collection_id)));
    }

    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_launch_collection_op(op.clone())
    };
    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "collection_id already pending launch, or failed standalone validation"));
    }

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewLaunchCollectionOp(op)).await;
    });

    Ok(Box::new(warp::reply::json(&QueuedResponse { status: "queued".to_string() })))
}

pub async fn handle_get_collection(
    collection_id: String,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let record = {
        let c = chain.lock().unwrap();
        c.collection_registry.get(&collection_id).cloned()
    };
    match record {
        Some(r) => Ok(Box::new(warp::reply::json(&r))),
        None => Ok(error_reply(StatusCode::NOT_FOUND, format!("collection '{}' is not launched", collection_id))),
    }
}

#[derive(Deserialize)]
pub struct CollectionsListQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

pub async fn handle_list_collections(
    query: CollectionsListQuery,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let limit = query.limit.unwrap_or(50).min(500);
    let mut records: Vec<CollectionRecord> = {
        let c = chain.lock().unwrap();
        c.collection_registry.values().cloned().collect()
    };
    records.sort_by(|a, b| b.launched_at_block.cmp(&a.launched_at_block).then(a.collection_id.cmp(&b.collection_id)));
    records.truncate(limit);
    Ok(Box::new(warp::reply::json(&records)))
}
