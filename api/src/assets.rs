//! HTTP surface for the Haze Asset Registry / NFTs (see core::assets).
//! Mints/transfers go through the mempool + P2P gossip like a normal
//! operation, since they're only actually committed once a block including
//! them is applied - these handlers just accept/queue/broadcast, they don't
//! mutate ChainState directly.
use std::sync::{Arc, Mutex};
use haze_chain::sync::LockExt;
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use haze_chain::chain::ChainState;
use haze_chain::mempool::Mempool;
use haze_chain::assets::{MintAssetOp, TransferAssetOp, AssetRecord};
use haze_crypto::pedersen::Commitment;
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
        let c = chain.lock_recover();
        c.asset_registry.contains_key(&op.asset_id)
    };
    if already_minted {
        return Ok(error_reply(StatusCode::CONFLICT, format!("asset '{}' is already minted", op.asset_id)));
    }

    let added = {
        let mut mp = mempool.lock_recover();
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
        let c = chain.lock_recover();
        c.asset_registry.get(&op.asset_id).cloned()
    };
    let Some(current) = current else {
        return Ok(error_reply(StatusCode::NOT_FOUND, format!("asset '{}' is not minted", op.asset_id)));
    };

    let msg = TransferAssetOp::signing_message(&op.asset_id, &op.new_owner_pubkey, &op.required_kernel_excess, &op.required_royalty_kernel_excess);
    if !op.signature.verify(&msg, &current.owner_pubkey) {
        return Ok(error_reply(StatusCode::FORBIDDEN, "signature does not match the asset's current owner"));
    }

    // Fast feedback for the marketplace atomic-swap primitive: if this
    // transfer is conditioned on a payment kernel, check now whether that
    // kernel actually exists yet, rather than only failing silently at
    // block-assembly time. Not a hard gate (the mempool itself still
    // accepts optimistically, matching the existing pattern for anything
    // whose validity depends on chain state) - apply_linear_block remains
    // the sole real enforcement point.
    if let Some(required_excess) = op.required_kernel_excess {
        let satisfied = {
            let c = chain.lock_recover();
            c.kernel_excesses.contains(&required_excess)
        };
        if !satisfied {
            return Ok(error_reply(StatusCode::CONFLICT, "required_kernel_excess does not exist on-chain yet - broadcast the payment transaction first"));
        }
    }

    // Same fast-feedback idea for the independent royalty condition (see
    // TransferAssetOp::required_royalty_kernel_excess) - not a hard gate,
    // just an earlier, clearer error than a silent drop at block-assembly
    // time. Only applies to an actual sale (required_kernel_excess is Some)
    // - an unconditional transfer (a gift, moving an asset between your own
    // wallets) has no sale price to take a cut of.
    if op.required_kernel_excess.is_some() {
        if let Some(collection_id) = &current.collection_id {
            let c = chain.lock_recover();
            if let Some(collection) = c.collection_registry.get(collection_id) {
                if collection.royalty_bps > 0 {
                    match op.required_royalty_kernel_excess {
                        None => return Ok(error_reply(StatusCode::BAD_REQUEST, "this collection charges a royalty - required_royalty_kernel_excess must be set")),
                        Some(required_royalty) if !c.kernel_excesses.contains(&required_royalty) => {
                            return Ok(error_reply(StatusCode::CONFLICT, "required_royalty_kernel_excess does not exist on-chain yet - broadcast the royalty payment first"));
                        }
                        Some(_) => {}
                    }
                }
            }
        }
    }

    let added = {
        let mut mp = mempool.lock_recover();
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
        let c = chain.lock_recover();
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
        let c = chain.lock_recover();
        c.asset_registry.values()
            .filter(|r| owner_filter.map(|o| r.owner_pubkey == o).unwrap_or(true))
            .cloned().collect()
    };
    records.sort_by(|a, b| b.minted_at_block.cmp(&a.minted_at_block).then(a.asset_id.cmp(&b.asset_id)));
    records.truncate(limit);
    Ok(Box::new(warp::reply::json(&records)))
}
