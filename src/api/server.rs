use warp::Filter;
use std::sync::{Arc, Mutex};
use std::convert::Infallible;
use serde::{Serialize, Deserialize};

use crate::core::mempool::Mempool;
use crate::core::chain::ChainState;
use crate::core::transaction::Transaction;
use crate::p2p::server::P2pServer;

pub struct ApiServer;

impl ApiServer {
    pub async fn start(
        mempool: Arc<Mutex<Mempool>>,
        chain: Arc<Mutex<ChainState>>,
        p2p_server: Arc<P2pServer>,
        port: u16,
    ) {
        let mempool_filter = warp::any().map(move || Arc::clone(&mempool));
        let chain_filter = warp::any().map(move || Arc::clone(&chain));
        let p2p_filter = warp::any().map(move || Arc::clone(&p2p_server));

        // POST /v1/transactions
        let tx_route = warp::post()
            .and(warp::path!("v1" / "transactions"))
            .and(warp::body::json())
            .and(mempool_filter)
            .and_then(handle_submit_transaction);

        // POST /v1/stake
        let stake_route = warp::post()
            .and(warp::path!("v1" / "stake"))
            .and(warp::body::json())
            .and(chain_filter.clone())
            .and(p2p_filter)
            .and_then(handle_register_validator);

        // GET /v1/utxos
        let utxos_route = warp::get()
            .and(warp::path!("v1" / "utxos"))
            .and(chain_filter)
            .and_then(handle_list_utxos);

        let routes = tx_route.or(stake_route).or(utxos_route).with(warp::cors().allow_any_origin());
        
        warp::serve(routes)
            .run(([127, 0, 0, 1], port))
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
) -> Result<impl warp::Reply, Infallible> {
    let registered = {
        let mut c = chain.lock().unwrap();
        c.register_validator(req.commitment, req.value, req.blinding)
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
