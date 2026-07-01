pub mod core;
pub mod crypto;
pub mod p2p;
pub mod api;
pub mod wallet;

use std::sync::{Arc, Mutex};
use clap::Parser;

use crate::core::mempool::Mempool;
use crate::core::chain::ChainState;
use crate::core::proposer::Proposer;
use crate::core::storage::Storage;
use crate::p2p::server::P2pServer;
use crate::api::server::ApiServer;
use crate::wallet::cli::{Cli, Commands, Wallet};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Node { bind, peers, rpc_port, stake_key } => {
            println!("Initializing Haze Node...");
            Storage::init();

            let state = Storage::load_state().unwrap_or_else(|| {
                println!("No previous state found. Starting fresh with Genesis Block.");
                let mut fresh_state = ChainState::new();
                let genesis = crate::core::genesis::genesis_block();
                fresh_state.apply_block(&genesis);
                fresh_state
            });

            let chain = Arc::new(Mutex::new(state));
            let mempool = Arc::new(Mutex::new(Mempool::new()));

            let key = stake_key.as_ref().map(|s| {
                curve25519_dalek_ng::scalar::Scalar::from(s.parse::<u64>().expect("Staking key must be a valid decimal number"))
            });

            let server = Arc::new(P2pServer::new(Arc::clone(&mempool), Arc::clone(&chain)));
            let proposer = Arc::new(Proposer::new(Arc::clone(&mempool), Arc::clone(&chain), key));

            // Link proposer to P2P server for block broadcasting
            proposer.set_p2p_server(Arc::clone(&server));

            println!("Starting Background Proposer...");
            let proposer_clone = Arc::clone(&proposer);
            tokio::spawn(async move {
                proposer_clone.start_proposing().await;
            });

            let rpc_mempool = Arc::clone(&mempool);
            let rpc_chain = Arc::clone(&chain);
            let rpc_server = Arc::clone(&server);
            let port = *rpc_port;
            println!("Starting HTTP JSON-RPC Server on 127.0.0.1:{}...", port);
            tokio::spawn(async move {
                ApiServer::start(rpc_mempool, rpc_chain, rpc_server, port).await;
            });

            let seed_peers: Vec<String> = peers.as_ref()
                .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
                .unwrap_or_default();

            println!("Starting P2P Network...");
            server.start(bind, seed_peers).await?;
        }
        Commands::Init { claim_genesis } => {
            Wallet::init(*claim_genesis)?;
        }
        Commands::Balance { rpc_port } => {
            Wallet::balance(*rpc_port).await?;
        }
        Commands::Send { amount, rpc_port } => {
            Wallet::send(*amount, *rpc_port).await?;
        }
        Commands::Stake { value, blinding, rpc_port } => {
            Wallet::stake(*value, *blinding, *rpc_port).await?;
        }
    }

    Ok(())
}
