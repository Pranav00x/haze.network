use warp::Filter;
use std::sync::{Arc, Mutex};
use std::convert::Infallible;
use serde::Serialize;

use crate::core::mempool::Mempool;
use crate::core::transaction::Transaction;

pub struct ApiServer;

impl ApiServer {
    pub async fn start(mempool: Arc<Mutex<Mempool>>, port: u16) {
        // Clone mempool for the warp filter
        let mempool_filter = warp::any().map(move || Arc::clone(&mempool));

        // POST /v1/transactions
        let tx_route = warp::post()
            .and(warp::path!("v1" / "transactions"))
            .and(warp::body::json())
            .and(mempool_filter)
            .and_then(handle_submit_transaction);

        let routes = tx_route.with(warp::cors().allow_any_origin());
        
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
