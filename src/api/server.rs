use warp::Filter;
use std::sync::{Arc, Mutex};
use std::convert::Infallible;
use serde::{Serialize, Deserialize};

use crate::core::mempool::Mempool;
use crate::core::chain::ChainState;
use crate::core::storage::Storage;
use crate::core::transaction::Transaction;
use crate::p2p::server::P2pServer;
use super::explorer;

pub struct ApiServer;

impl ApiServer {
    pub async fn start(
        mempool: Arc<Mutex<Mempool>>,
        chain: Arc<Mutex<ChainState>>,
        p2p_server: Arc<P2pServer>,
        storage: Arc<Storage>,
        port: u16,
    ) {
        let mempool_filter = warp::any().map(move || Arc::clone(&mempool));
        let chain_filter = warp::any().map(move || Arc::clone(&chain));
        let p2p_filter = warp::any().map(move || Arc::clone(&p2p_server));
        let storage_filter = warp::any().map(move || Arc::clone(&storage));
        let mempool_filter_2 = mempool_filter.clone();

        // Caps request body size for the two write endpoints - now that this
        // API is meant to be internet-facing, an unbounded body from an
        // untrusted caller is the same class of problem MAX_MESSAGE_SIZE
        // guards against on the P2P side (src/p2p/server.rs).
        const MAX_BODY_SIZE: u64 = 1024 * 1024; // 1MB

        // POST /v1/transactions
        let tx_route = warp::post()
            .and(warp::path!("v1" / "transactions"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(mempool_filter)
            .and_then(handle_submit_transaction);

        // POST /v1/stake
        let stake_route = warp::post()
            .and(warp::path!("v1" / "stake"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(chain_filter.clone())
            .and(p2p_filter)
            .and(storage_filter)
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
            .and(chain_filter)
            .and_then(explorer::handle_search);

        let routes = tx_route
            .or(stake_route)
            .or(utxos_route)
            .or(index_route)
            .or(status_route)
            .or(blocks_list_route)
            .or(block_detail_route)
            .or(validators_route)
            .or(transactions_route)
            .or(search_route)
            .with(warp::cors().allow_any_origin());
        
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
    blinding: curve25519_dalek_ng::scalar::Scalar,
}

async fn handle_submit_transaction(
    tx: Transaction,
    mempool: Arc<Mutex<Mempool>>,
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
        mp.add_transaction(tx)
    };

    if added {
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

async fn handle_register_validator(
    req: StakeRequest,
    chain: Arc<Mutex<ChainState>>,
    p2p_server: Arc<P2pServer>,
    storage: Arc<Storage>,
) -> Result<impl warp::Reply, Infallible> {
    let registered = {
        let mut c = chain.lock().unwrap();
        let ok = c.register_validator(req.commitment, req.value, req.blinding);
        if ok {
            if let Err(e) = storage.persist_active_validators(&c.active_validators) {
                println!("Warning: Failed to persist validator registration: {}", e);
            }
        }
        ok
    };

    if registered {
        let pm = Arc::clone(&p2p_server.peer_manager);
        let msg = crate::p2p::message::P2pMessage::RegisterValidator {
            commitment: req.commitment,
            value: req.value,
            blinding: req.blinding,
        };
        tokio::spawn(async move {
            pm.broadcast(&msg).await;
        });

        let response = ApiResponse {
            status: "success".to_string(),
            message: "Validator registered and propagated successfully".to_string(),
        };
        Ok(warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::OK))
    } else {
        let response = ApiResponse {
            status: "error".to_string(),
            message: "Validator registration failed (invalid parameters, UTXO spent, or already registered)".to_string(),
        };
        Ok(warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::BAD_REQUEST))
    }
}
