pub mod core;
pub mod crypto;
pub mod p2p;
pub mod wallet;

use std::sync::{Arc, Mutex};
use clap::Parser;

use crate::core::mempool::Mempool;
use crate::core::chain::ChainState;
use crate::core::miner::Miner;
use crate::core::storage::Storage;
use crate::p2p::server::P2pServer;
use crate::wallet::cli::{Cli, Commands, Wallet};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Node => {
            println!("Initializing Haze Node...");
            Storage::init();

            let state = Storage::load_state().unwrap_or_else(|| {
                println!("No previous state found. Starting fresh.");
                ChainState::new()
            });

            let chain = Arc::new(Mutex::new(state));
            let mempool = Arc::new(Mutex::new(Mempool::new()));
            
            let server = P2pServer::new(Arc::clone(&mempool));
            let miner = Miner::new(Arc::clone(&mempool), Arc::clone(&chain));

            println!("Starting Background Miner...");
            tokio::spawn(async move {
                miner.start_mining().await;
            });

            println!("Starting P2P Network...");
            server.start("127.0.0.1:8333").await?;
        }
        Commands::Send { amount } => {
            Wallet::send_dummy_transaction(*amount).await?;
        }
    }

    Ok(())
}
