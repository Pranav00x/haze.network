pub mod core;
pub mod crypto;
pub mod p2p;

use std::sync::{Arc, Mutex};
use crate::core::mempool::Mempool;
use crate::p2p::server::P2pServer;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!("Initializing Haze Node...");

    let mempool = Arc::new(Mutex::new(Mempool::new()));
    let server = P2pServer::new(mempool);

    println!("Starting P2P Network...");
    server.start("127.0.0.1:8333").await?;

    Ok(())
}
