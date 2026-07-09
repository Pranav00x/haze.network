use warp::Filter;
use std::sync::{Arc, Mutex};
use std::convert::Infallible;
use std::net::SocketAddr;
use serde::{Serialize, Deserialize};

use crate::core::mempool::Mempool;
use crate::core::chain::ChainState;
use crate::core::storage::Storage;
use crate::core::transaction::Transaction;
use crate::core::marketplace::MarketplaceState;
use crate::core::allowlist::AllowlistState;
use crate::p2p::server::P2pServer;
use super::explorer;
use super::faucet::{self, FaucetState};
use super::names;
use super::assets;
use super::marketplace;
use super::collections;
use super::allowlist;
use super::inbox::{self, InboxState};

pub struct ApiServer;

impl ApiServer {
    pub async fn start(
        mempool: Arc<Mutex<Mempool>>,
        chain: Arc<Mutex<ChainState>>,
        p2p_server: Arc<P2pServer>,
        storage: Arc<Storage>,
        marketplace_state: Arc<MarketplaceState>,
        allowlist_state: Arc<AllowlistState>,
        port: u16,
    ) {
        let faucet_state = {
            let snapshot = chain.lock().unwrap();
            Arc::new(FaucetState::new(&snapshot))
        };

        let mempool_filter = warp::any().map(move || Arc::clone(&mempool));
        let chain_filter = warp::any().map(move || Arc::clone(&chain));
        let p2p_filter = warp::any().map(move || Arc::clone(&p2p_server));
        // storage is no longer touched directly by this module - stake
        // registration now flows through the mempool/block pipeline like
        // every other op, so there's nothing left here to persist outside
        // of normal block application (see core::storage::Storage::
        // persist_applied, already called from the proposer/p2p paths).
        let _ = &storage;
        let mempool_filter_2 = mempool_filter.clone();

        let faucet_filter = warp::any().map(move || Arc::clone(&faucet_state));
        let faucet_filter_2 = faucet_filter.clone();
        let faucet_filter_3 = faucet_filter.clone();
        let mempool_filter_3 = mempool_filter.clone();
        let mempool_filter_4 = mempool_filter.clone();
        let mempool_filter_5 = mempool_filter.clone();
        let mempool_filter_6 = mempool_filter.clone();
        let mempool_filter_7 = mempool_filter.clone();
        let mempool_filter_8 = mempool_filter.clone();
        let mempool_filter_9 = mempool_filter.clone();
        let p2p_filter_2 = p2p_filter.clone();
        let p2p_filter_3 = p2p_filter.clone();
        let p2p_filter_4 = p2p_filter.clone();
        let p2p_filter_5 = p2p_filter.clone();
        let p2p_filter_6 = p2p_filter.clone();
        let p2p_filter_7 = p2p_filter.clone();
        let p2p_filter_8 = p2p_filter.clone();

        let inbox_state = Arc::new(InboxState::new());
        let inbox_filter = warp::any().map(move || Arc::clone(&inbox_state));
        let inbox_filter_2 = inbox_filter.clone();

        let marketplace_filter = warp::any().map(move || Arc::clone(&marketplace_state));
        let marketplace_filter_2 = marketplace_filter.clone();
        let marketplace_filter_3 = marketplace_filter.clone();
        let marketplace_filter_4 = marketplace_filter.clone();
        let p2p_filter_9 = p2p_filter.clone();
        let p2p_filter_10 = p2p_filter.clone();

        let allowlist_filter = warp::any().map(move || Arc::clone(&allowlist_state));
        let allowlist_filter_2 = allowlist_filter.clone();
        let mempool_filter_10 = mempool_filter.clone();
        let p2p_filter_11 = p2p_filter.clone();
        let p2p_filter_12 = p2p_filter.clone();
        let mempool_filter_11 = mempool_filter.clone();

        // Caps request body size for the two write endpoints - now that this
        // API is meant to be internet-facing, an unbounded body from an
        // untrusted caller is the same class of problem MAX_MESSAGE_SIZE
        // guards against on the P2P side (src/p2p/server.rs).
        const MAX_BODY_SIZE: u64 = 1024 * 1024; // 1MB

        // POST /v1/transactions - queued into the local mempool immediately
        // (for a fast accept/reject response to the wallet), then routed
        // into Dandelion++ stem/fluff the same way any relayed transaction
        // is (see p2p::server::dispatch_dandelion_tx) so it actually
        // reaches the network instead of sitting only in this node's
        // mempool forever if this node never happens to be the proposer.
        let tx_route = warp::post()
            .and(warp::path!("v1" / "transactions"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter)
            .and(p2p_filter_8)
            .and_then(handle_submit_transaction);

        // POST /v1/stake
        let stake_route = warp::post()
            .and(warp::path!("v1" / "stake"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter_11)
            .and(p2p_filter)
            .and_then(handle_register_validator);

        // GET /v1/utxos
        let utxos_route = warp::get()
            .and(warp::path!("v1" / "utxos"))
            .and(chain_filter.clone())
            .and_then(handle_list_utxos);

        // GET /
        let index_route = warp::get()
            .and(warp::path::end())
            .and_then(explorer::handle_index);

        // GET /v1/status
        let status_route = warp::get()
            .and(warp::path!("v1" / "status"))
            .and(chain_filter.clone())
            .and(mempool_filter_2)
            .and_then(explorer::handle_status);

        // GET /v1/scan-outputs - every on-chain output carrying a
        // recoverable note, for a restored wallet to try decrypting (see
        // wallet::note and explorer::handle_scan_outputs).
        let scan_outputs_route = warp::get()
            .and(warp::path!("v1" / "scan-outputs"))
            .and(chain_filter.clone())
            .and_then(explorer::handle_scan_outputs);

        // GET /v1/fee-estimate - fixed, size-based fee suggestion (see
        // Mempool::suggested_fee) - wallets should call this instead of
        // hardcoding the constant directly.
        let fee_estimate_route = warp::get()
            .and(warp::path!("v1" / "fee-estimate"))
            .and(mempool_filter_7)
            .and_then(explorer::handle_fee_estimate);

        // GET /v1/blocks?limit=N
        let blocks_list_route = warp::get()
            .and(warp::path!("v1" / "blocks"))
            .and(warp::query::<explorer::BlocksQuery>())
            .and(chain_filter.clone())
            .and_then(explorer::handle_blocks_list);

        // GET /v1/blocks/:height
        let block_detail_route = warp::get()
            .and(warp::path!("v1" / "blocks" / u64))
            .and(chain_filter.clone())
            .and_then(explorer::handle_block_detail);

        // GET /v1/validators
        let validators_route = warp::get()
            .and(warp::path!("v1" / "validators"))
            .and(chain_filter.clone())
            .and_then(explorer::handle_validators);

        // GET /v1/transactions?limit=N
        let transactions_route = warp::get()
            .and(warp::path!("v1" / "transactions"))
            .and(warp::query::<explorer::BlocksQuery>())
            .and(chain_filter.clone())
            .and_then(explorer::handle_transactions_list);

        // GET /v1/search?q=...
        let search_route = warp::get()
            .and(warp::path!("v1" / "search"))
            .and(warp::query::<explorer::SearchQuery>())
            .and(chain_filter.clone())
            .and_then(explorer::handle_search);

        // POST /v1/faucet - devnet-only repeatable faucet (see api/faucet.rs),
        // step 1: server builds a slate paying the requester from its own
        // faucet reserve, hands back the slate JSON to respond to. Rate
        // limited per requester IP (see faucet::client_ip) - X-Forwarded-For
        // first, since this node runs behind a reverse proxy in production.
        let faucet_request_route = warp::post()
            .and(warp::path!("v1" / "faucet"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(warp::header::optional::<String>("x-forwarded-for"))
            .and(warp::addr::remote())
            .and(faucet_filter)
            .and(chain_filter.clone())
            .and_then(faucet::handle_faucet_request);

        // POST /v1/faucet/complete - step 2: server finalizes with the
        // requester's response and broadcasts the resulting transaction.
        let faucet_complete_route = warp::post()
            .and(warp::path!("v1" / "faucet" / "complete"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(faucet_filter_2)
            .and(mempool_filter_3)
            .and_then(faucet::handle_faucet_complete);

        // POST /v1/names/register - queues a name registration (see api/names.rs
        // and core::registry) into the mempool and broadcasts it; it's only
        // actually committed once a block including it is mined.
        let register_name_route = warp::post()
            .and(warp::path!("v1" / "names" / "register"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter_4)
            .and(p2p_filter_2)
            .and(chain_filter.clone())
            .and_then(names::handle_register_name);

        // POST /v1/names/transfer - queues a name ownership transfer, signed
        // by the name's current owner (see api/names.rs).
        let transfer_name_route = warp::post()
            .and(warp::path!("v1" / "names" / "transfer"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter_5)
            .and(p2p_filter_3)
            .and(chain_filter.clone())
            .and_then(names::handle_transfer_name);

        // POST /v1/names/register-sponsored - same as register, except the
        // flat registration fee is paid by the node's own faucet reserve
        // instead of the requester (see api/names.rs and
        // FaucetState::build_sponsored_fee_payment). Lets a brand-new
        // zero-balance wallet still claim a name on mainnet, where there's no
        // general-purpose faucet to fund it first.
        let sponsored_register_name_route = warp::post()
            .and(warp::path!("v1" / "names" / "register-sponsored"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(faucet_filter_3)
            .and(mempool_filter_6)
            .and(p2p_filter_4)
            .and(chain_filter.clone())
            .and_then(names::handle_sponsored_register_name);

        // GET /v1/names/:name - resolves a single registered name.
        let resolve_name_route = warp::get()
            .and(warp::path!("v1" / "names" / String))
            .and(chain_filter.clone())
            .and_then(names::handle_resolve_name);

        // GET /v1/names?limit=N - lists registered names, newest first.
        let list_names_route = warp::get()
            .and(warp::path!("v1" / "names"))
            .and(warp::query::<names::NamesListQuery>())
            .and(chain_filter.clone())
            .and_then(names::handle_list_names);

        // POST /v1/assets/mint - queues an NFT mint (see api/assets.rs and
        // core::assets) into the mempool and broadcasts it - separate
        // namespace from /v1/names, same shape.
        let mint_asset_route = warp::post()
            .and(warp::path!("v1" / "assets" / "mint"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter_8)
            .and(p2p_filter_5)
            .and(chain_filter.clone())
            .and_then(assets::handle_mint_asset);

        // POST /v1/assets/transfer - queues an NFT ownership transfer,
        // signed by the asset's current owner.
        let transfer_asset_route = warp::post()
            .and(warp::path!("v1" / "assets" / "transfer"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter_9)
            .and(p2p_filter_6)
            .and(chain_filter.clone())
            .and_then(assets::handle_transfer_asset);

        // GET /v1/assets/:asset_id - resolves a single minted asset.
        let resolve_asset_route = warp::get()
            .and(warp::path!("v1" / "assets" / String))
            .and(chain_filter.clone())
            .and_then(assets::handle_resolve_asset);

        // GET /v1/assets?limit=N - lists minted assets, newest first.
        let list_assets_route = warp::get()
            .and(warp::path!("v1" / "assets"))
            .and(warp::query::<assets::AssetsListQuery>())
            .and(chain_filter.clone())
            .and_then(assets::handle_list_assets);

        // POST /v1/marketplace/list - creates or replaces a listing (see
        // api/marketplace.rs and core::marketplace). Not part of consensus -
        // no mempool involvement, just MarketplaceState + P2P gossip.
        let create_listing_route = warp::post()
            .and(warp::path!("v1" / "marketplace" / "list"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(marketplace_filter)
            .and(p2p_filter_9)
            .and(chain_filter.clone())
            .and_then(marketplace::handle_create_listing);

        // POST /v1/marketplace/cancel
        let cancel_listing_route = warp::post()
            .and(warp::path!("v1" / "marketplace" / "cancel"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(marketplace_filter_2)
            .and(p2p_filter_10)
            .and_then(marketplace::handle_cancel_listing);

        // GET /v1/marketplace/listings
        let list_listings_route = warp::get()
            .and(warp::path!("v1" / "marketplace" / "listings"))
            .and(marketplace_filter_3)
            .and_then(marketplace::handle_list_listings);

        // GET /v1/marketplace/listings/:asset_id
        let get_listing_route = warp::get()
            .and(warp::path!("v1" / "marketplace" / "listings" / String))
            .and(marketplace_filter_4)
            .and_then(marketplace::handle_get_listing);

        // POST /v1/collections/launch - queues a scheduled multi-phase drop
        // launch (see api/collections.rs and core::collections) into the
        // mempool and broadcasts it - same shape as mint_asset_route.
        let launch_collection_route = warp::post()
            .and(warp::path!("v1" / "collections" / "launch"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter_10)
            .and(p2p_filter_11)
            .and(chain_filter.clone())
            .and_then(collections::handle_launch_collection);

        // GET /v1/collections - lists launched collections, newest first.
        let list_collections_route = warp::get()
            .and(warp::path!("v1" / "collections"))
            .and(warp::query::<collections::CollectionsListQuery>())
            .and(chain_filter.clone())
            .and_then(collections::handle_list_collections);

        // GET /v1/collections/:collection_id - resolves a single launched collection.
        let get_collection_route = warp::get()
            .and(warp::path!("v1" / "collections" / String))
            .and(chain_filter.clone())
            .and_then(collections::handle_get_collection);

        // POST /v1/collections/:collection_id/phases/:phase_index/allowlist -
        // publishes the full plaintext allowlist for one phase (see
        // api/allowlist.rs and core::allowlist). Not part of consensus - only
        // the phase's Merkle root is; this just makes the underlying list
        // fetchable so any client can compute its own inclusion proof.
        let publish_allowlist_route = warp::post()
            .and(warp::path!("v1" / "collections" / String / "phases" / u32 / "allowlist"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .map(|_collection_id: String, _phase_index: u32, entry: crate::core::allowlist::AllowlistEntry| entry)
            .and(allowlist_filter)
            .and(p2p_filter_12)
            .and(chain_filter.clone())
            .and_then(allowlist::handle_publish_allowlist);

        // GET /v1/collections/:collection_id/phases/:phase_index/allowlist
        let get_allowlist_route = warp::get()
            .and(warp::path!("v1" / "collections" / String / "phases" / u32 / "allowlist"))
            .and(allowlist_filter_2)
            .and_then(allowlist::handle_get_allowlist);

        // GET /v1/p2p/ws - WebSocket P2P transport, for peers that can't be
        // dialed via raw TCP (see src/p2p/transport.rs for why this exists -
        // Render only proxies a single HTTP(S) port, so this rides on the
        // same port as the JSON API instead of needing 8333 exposed).
        let p2p_ws_route = warp::path!("v1" / "p2p" / "ws")
            .and(warp::ws())
            .and(warp::addr::remote())
            .and(p2p_filter_7)
            .map(|ws: warp::ws::Ws, remote: Option<SocketAddr>, p2p_server: Arc<P2pServer>| {
                let label = remote.map(|a| a.to_string()).unwrap_or_else(|| "ws-peer".to_string());
                ws.max_message_size(1024 * 1024 * 32)
                    .max_frame_size(1024 * 1024 * 32)
                    .on_upgrade(move |socket| async move {
                        p2p_server.handle_inbound_ws(socket, label).await;
                    })
            });

        // POST /v1/inbox/:pubkey_hex - drops a slate/response off for a
        // recipient (see api/inbox.rs). Not part of consensus - pure message
        // relay, so it needs neither the mempool nor the chain.
        let post_inbox_route = warp::post()
            .and(warp::path!("v1" / "inbox" / String))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(inbox_filter)
            .and_then(inbox::handle_post_inbox);

        // GET /v1/inbox/:pubkey_hex - drains and returns pending messages.
        let get_inbox_route = warp::get()
            .and(warp::path!("v1" / "inbox" / String))
            .and(inbox_filter_2)
            .and_then(inbox::handle_get_inbox);

        let routes = tx_route
            .or(stake_route)
            .or(utxos_route)
            .or(index_route)
            .or(status_route)
            .or(scan_outputs_route)
            .or(fee_estimate_route)
            .or(blocks_list_route)
            .or(block_detail_route)
            .or(validators_route)
            .or(transactions_route)
            .or(search_route)
            .or(faucet_request_route)
            .or(faucet_complete_route)
            .or(register_name_route)
            .or(transfer_name_route)
            .or(sponsored_register_name_route)
            .or(resolve_name_route)
            .or(list_names_route)
            .or(mint_asset_route)
            .or(transfer_asset_route)
            .or(resolve_asset_route)
            .or(list_assets_route)
            .or(create_listing_route)
            .or(cancel_listing_route)
            .or(list_listings_route)
            .or(get_listing_route)
            .or(launch_collection_route)
            .or(list_collections_route)
            .or(get_collection_route)
            .or(publish_allowlist_route)
            .or(get_allowlist_route)
            .or(p2p_ws_route)
            .or(post_inbox_route)
            .or(get_inbox_route)
            .with(
                warp::cors()
                    .allow_any_origin()
                    // A plain GET never preflights, but a JSON POST always does
                    // (application/json isn't a "simple" content-type) - without
                    // these, the browser blocks the actual POST after the
                    // preflight response fails to allow the method/header.
                    .allow_methods(vec!["GET", "POST"])
                    .allow_headers(vec!["content-type"]),
            );

        // Binds all interfaces, not just loopback - required for this to be
        // reachable at all once deployed behind a cloud provider's proxy
        // (e.g. Fly.io), which connects over the network, not localhost.
        warp::serve(routes)
            .run(([0, 0, 0, 0], port))
            .await;
    }
}

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
}

#[derive(Deserialize, Serialize)]
struct StakeRequest {
    commitment: crate::crypto::pedersen::Commitment,
    value: u64,
    /// Proof of ownership - see ChainState::register_validator. NOT the raw
    /// blinding factor: the client signs this locally and only the
    /// resulting signature ever travels over the network.
    proof: crate::crypto::schnorr::Signature,
}

async fn handle_submit_transaction(
    tx: Transaction,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
) -> Result<impl warp::Reply, Infallible> {
    // Validate the transaction mathematically first
    if !tx.validate() {
        let response = ApiResponse {
            status: "error".to_string(),
            message: "Transaction failed cryptographic validation".to_string(),
        };
        return Ok(warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::BAD_REQUEST));
    }

    // Try to add to mempool
    let added = {
        let mut mp = mempool.lock().unwrap();
        mp.add_transaction(tx.clone())
    };

    if added {
        tokio::spawn(async move {
            p2p_server.propagate_new_transaction(tx, true).await;
        });

        let response = ApiResponse {
            status: "success".to_string(),
            message: "Transaction accepted into the mempool".to_string(),
        };
        Ok(warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::OK))
    } else {
        let response = ApiResponse {
            status: "error".to_string(),
            message: "Transaction rejected by mempool (duplicate or conflict)".to_string(),
        };
        Ok(warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::BAD_REQUEST))
    }
}

async fn handle_list_utxos(
    chain: Arc<Mutex<ChainState>>,
) -> Result<impl warp::Reply, Infallible> {
    let utxos: Vec<crate::crypto::pedersen::Commitment> = {
        let c = chain.lock().unwrap();
        c.utxos.iter().cloned().collect()
    };
    Ok(warp::reply::json(&utxos))
}

/// Queues a stake registration into the mempool (see core::chain::
/// RegisterValidatorOp) rather than mutating active_validators directly -
/// it only actually takes effect once a proposer includes it in a block,
/// same as every other op type here. This is what makes the resulting
/// validator set deterministic: every node derives it purely from block
/// content/order, never from whichever order registrations happened to
/// arrive over the network.
async fn handle_register_validator(
    req: StakeRequest,
    mempool: Arc<Mutex<Mempool>>,
    p2p_server: Arc<P2pServer>,
) -> Result<impl warp::Reply, Infallible> {
    let op = crate::core::chain::RegisterValidatorOp {
        commitment: req.commitment,
        value: req.value,
        proof: req.proof,
    };
    let queued = {
        let mut mp = mempool.lock().unwrap();
        mp.add_validator_op(op.clone())
    };

    if queued {
        let pm = Arc::clone(&p2p_server.peer_manager);
        let msg = crate::p2p::message::P2pMessage::NewValidatorOp(op);
        tokio::spawn(async move {
            pm.broadcast(&msg).await;
        });

        let response = ApiResponse {
            status: "success".to_string(),
            message: "Stake registration queued and propagated - takes effect once mined into a block".to_string(),
        };
        Ok(warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::OK))
    } else {
        let response = ApiResponse {
            status: "error".to_string(),
            message: "Stake registration rejected (invalid proof, below minimum stake, or already pending)".to_string(),
        };
        Ok(warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::BAD_REQUEST))
    }
}
