//! HTTP surface for marketplace listings (see core::marketplace). Listings
//! are pure discovery metadata, not consensus state - these handlers talk
//! directly to MarketplaceState, never to the mempool.
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use warp::http::StatusCode;

use haze_chain::chain::ChainState;
use haze_chain::marketplace::{Listing, MarketplaceState, cancel_signing_message};
use haze_crypto::pedersen::Commitment;
use haze_crypto::schnorr::Signature;
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

pub async fn handle_create_listing(
    listing: Listing,
    marketplace: Arc<MarketplaceState>,
    p2p_server: Arc<P2pServer>,
    chain: Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if let Err(e) = listing.validate_standalone() {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("invalid listing signature: {:?}", e)));
    }

    let ownership_ok = {
        let c = chain.lock().unwrap();
        listing.validate_against_registry(&c.asset_registry).is_ok()
    };
    if !ownership_ok {
        return Ok(error_reply(StatusCode::FORBIDDEN, "seller does not currently own this asset"));
    }

    marketplace.add_or_replace(listing.clone());

    let pm = Arc::clone(&p2p_server.peer_manager);
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::NewListing(listing)).await;
    });

    Ok(Box::new(warp::reply::json(&QueuedResponse { status: "listed".to_string() })))
}

#[derive(Deserialize)]
pub struct CancelListingRequest {
    pub asset_id: String,
    pub seller_pubkey: Commitment,
    pub signature: Signature,
}

pub async fn handle_cancel_listing(
    req: CancelListingRequest,
    marketplace: Arc<MarketplaceState>,
    p2p_server: Arc<P2pServer>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let msg = cancel_signing_message(&req.asset_id, &req.seller_pubkey);
    if !req.signature.verify(&msg, &req.seller_pubkey) {
        return Ok(error_reply(StatusCode::FORBIDDEN, "signature does not match seller_pubkey"));
    }

    if !marketplace.cancel(&req.asset_id, &req.seller_pubkey) {
        return Ok(error_reply(StatusCode::NOT_FOUND, "no matching listing to cancel"));
    }

    let pm = Arc::clone(&p2p_server.peer_manager);
    let asset_id = req.asset_id.clone();
    let seller_pubkey = req.seller_pubkey;
    let signature = req.signature;
    tokio::spawn(async move {
        pm.broadcast(&P2pMessage::CancelListing { asset_id, seller_pubkey, signature }).await;
    });

    Ok(Box::new(warp::reply::json(&QueuedResponse { status: "cancelled".to_string() })))
}

pub async fn handle_list_listings(
    marketplace: Arc<MarketplaceState>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let mut listings = marketplace.list_all();
    listings.sort_by(|a, b| b.listed_at.cmp(&a.listed_at).then(a.asset_id.cmp(&b.asset_id)));
    Ok(Box::new(warp::reply::json(&listings)))
}

pub async fn handle_get_listing(
    asset_id: String,
    marketplace: Arc<MarketplaceState>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    match marketplace.get(&asset_id) {
        Some(listing) => Ok(Box::new(warp::reply::json(&listing))),
        None => Ok(error_reply(StatusCode::NOT_FOUND, format!("no listing for asset '{}'", asset_id))),
    }
}
