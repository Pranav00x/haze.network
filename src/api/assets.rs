//! HTTP surface for the Haze Asset Registry / NFTs (see core::assets).
//! Mints/transfers go through the mempool + P2P gossip like a normal
//! operation, since they're only actually committed once a block including
//! them is applied - these handlers just accept/queue/broadcast, they don't
//! mutate ChainState directly.
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use crate::core::chain::ChainState;
use crate::core::mempool::Mempool;
use crate::core::assets::{MintAssetOp, TransferAssetOp, AssetRecord};
use crate::crypto::pedersen::Commitment;
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
struct QueuedResponse {
    status: String,
}

pub async fn handle_mint_asset(
    op: MintAssetOp,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if let Err(e) = op.validate_standalone() {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("invalid mint: {:?}", e)));
    }

    // Reject already-minted asset_ids immediately, rather than accepting a
    // "queued" response for something the proposer will silently drop at
    // block-assembly time and that will never actually get mined.
    let already_minted = {
        let c = chain.lock().unwrap();
        c.asset_registry.contains_key(&op.asset_id)
    };
    if already_minted {
        return Ok(error_reply(StatusCode::CONFLICT, format!("asset '{}' is already minted", op.asset_id)));
    }

    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_mint_op(op.clone())
    };

    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "asset_id already pending mint or has an unresolvable fee-payment input"));
    }

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewMintOp(op)).await;
    });

    Ok(Box::new(warp::reply::json(&QueuedResponse { status: "queued".to_string() })))
}

pub async fn handle_transfer_asset(
    op: TransferAssetOp,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let current = {
        let c = chain.lock().unwrap();
        c.asset_registry.get(&op.asset_id).cloned()
    };
    let Some(current) = current else {
        return Ok(error_reply(StatusCode::NOT_FOUND, format!("asset '{}' is not minted", op.asset_id)));
    };

    let msg = TransferAssetOp::signing_message(&op.asset_id, &op.new_owner_pubkey);
    if !op.signature.verify(&msg, &current.owner_pubkey) {
        return Ok(error_reply(StatusCode::FORBIDDEN, "signature does not match the asset's current owner"));
    }

    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_transfer_asset_op(op.clone())
    };
    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "asset already has a pending transfer"));
    }

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewTransferAssetOp(op)).await;
    });

    Ok(Box::new(warp::reply::json(&QueuedResponse { status: "queued".to_string() })))
}

pub async fn handle_resolve_asset(
    asset_id: String,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let record = {
        let c = chain.lock().unwrap();
        c.asset_registry.get(&asset_id).cloned()
    };
    match record {
        Some(r) => Ok(Box::new(warp::reply::json(&r))),
        None => Ok(error_reply(StatusCode::NOT_FOUND, format!("asset '{}' is not minted", asset_id))),
    }
}

#[derive(Deserialize)]
pub struct AssetsListQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    /// Filters to assets owned by this identity pubkey (hex) - same
    /// rediscovery purpose as NamesListQuery::owner (see its doc comment):
    /// restore-from-phrase recovers the identity_key itself, so a restored
    /// wallet can ask the chain directly which assets it owns instead of
    /// needing to remember every asset_id it ever minted.
    #[serde(default)]
    pub owner: Option<String>,
}

pub async fn handle_list_assets(
    query: AssetsListQuery,
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
    let mut records: Vec<AssetRecord> = {
        let c = chain.lock().unwrap();
        c.asset_registry.values()
            .filter(|r| owner_filter.map(|o| r.owner_pubkey == o).unwrap_or(true))
            .cloned().collect()
    };
    records.sort_by(|a, b| b.minted_at_block.cmp(&a.minted_at_block).then(a.asset_id.cmp(&b.asset_id)));
    records.truncate(limit);
    Ok(Box::new(warp::reply::json(&records)))
}
