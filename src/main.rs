pub mod core;
pub mod crypto;
pub mod p2p;

use std::sync::{Arc, Mutex};
use crate::core::mempool::Mempool;
use crate::core::chain::ChainState;
use crate::core::miner::Miner;
use crate::p2p::server::P2pServer;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!("Initializing Haze Node...");

    let chain = Arc::new(Mutex::new(ChainState::new()));
    let mempool = Arc::new(Mutex::new(Mempool::new()));
    
    let server = P2pServer::new(Arc::clone(&mempool));
    let miner = Miner::new(Arc::clone(&mempool), Arc::clone(&chain));

    println!("Starting Background Miner...");
    tokio::spawn(async move {
        miner.start_mining().await;
    });

    println!("Starting P2P Network...");
    server.start("127.0.0.1:8333").await?;

    Ok(())
}
