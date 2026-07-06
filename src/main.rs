// Raised from the default (128) - warp's Or<> combinator nests one generic
// layer per route joined via .or(), and with ~25 routes now registered
// (see api::server::ApiServer::start), the compiler's default recursion
// limit for proving the resulting filter's Future: Unpin is exceeded.
#![recursion_limit = "512"]

use std::sync::{Arc, Mutex};
use clap::Parser;

use haze_core::core::mempool::Mempool;
use haze_core::core::proposer::Proposer;
use haze_core::core::storage::Storage;
use haze_core::core::chain::ApplyResult;
use haze_core::core::compaction::Compactor;
use haze_core::core::genesis;
use haze_core::core::marketplace::MarketplaceState;
use haze_core::p2p::server::P2pServer;
use haze_core::api::server::ApiServer;
use haze_core::wallet::cli::{Cli, Commands, Wallet};

/// Accepts either a plain decimal (e.g. the well-known devnet keys 42/43) or
/// a 64-char hex-encoded scalar (the raw blinding revealed by the web
/// wallet's "reveal stake key" flow) - a wallet-derived blinding is a full
/// 256-bit scalar, not representable as a small decimal.
fn parse_stake_key(s: &str) -> curve25519_dalek_ng::scalar::Scalar {
    use curve25519_dalek_ng::scalar::Scalar;
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .expect("Staking key hex must be valid");
        }
        Scalar::from_bits(bytes)
    } else {
        Scalar::from(s.parse::<u64>().expect("Staking key must be a decimal number or a 64-char hex string"))
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Node { bind, peers, rpc_port, stake_key } => {
            println!("Initializing Haze Node...");
            let storage = Arc::new(Storage::open());

            let mut state = storage.load_state();
            if state.current_height == 0 && state.last_block_hash == [0u8; 32] && state.blocks.is_empty() {
                println!("No previous state found. Starting fresh with Genesis Block.");
                let genesis = genesis::genesis_block();
                if let ApplyResult::Linear(delta) = state.apply_block(&genesis) {
                    if let Err(e) = storage.persist_applied(&delta) {
                        println!("Warning: Failed to persist genesis block: {}", e);
                    }
                }
            } else {
                println!("Resumed chain state from disk at height {}.", state.current_height);
            }

            let chain = Arc::new(Mutex::new(state));
            let mempool = Arc::new(Mutex::new(Mempool::new()));

            let key = stake_key.as_ref().map(|s| parse_stake_key(s));

            let marketplace_state = Arc::new(MarketplaceState::new());

            let server = Arc::new(P2pServer::new(Arc::clone(&mempool), Arc::clone(&chain), Arc::clone(&storage), Arc::clone(&marketplace_state)));
            let proposer = Arc::new(Proposer::new(Arc::clone(&mempool), Arc::clone(&chain), Arc::clone(&storage), key));

            // Link proposer to P2P server for block broadcasting
            proposer.set_p2p_server(Arc::clone(&server));

            println!("Starting Background Proposer...");
            let proposer_clone = Arc::clone(&proposer);
            tokio::spawn(async move {
                proposer_clone.start_proposing().await;
            });

            let compactor = Arc::new(Compactor::new(Arc::clone(&chain), Arc::clone(&storage)));
            tokio::spawn(async move {
                compactor.run_periodic().await;
            });

            let rpc_mempool = Arc::clone(&mempool);
            let rpc_chain = Arc::clone(&chain);
            let rpc_server = Arc::clone(&server);
            let rpc_storage = Arc::clone(&storage);
            let rpc_marketplace = Arc::clone(&marketplace_state);
            let port = *rpc_port;
            println!("Starting HTTP JSON-RPC Server on 127.0.0.1:{}...", port);
            tokio::spawn(async move {
                ApiServer::start(rpc_mempool, rpc_chain, rpc_server, rpc_storage, rpc_marketplace, port).await;
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
        Commands::Pay { amount, slate_out, rpc_port } => {
            Wallet::pay(*amount, slate_out.clone(), *rpc_port).await?;
        }
        Commands::Receive { slate_in, slate_out } => {
            Wallet::receive(slate_in.clone(), slate_out.clone()).await?;
        }
        Commands::Complete { slate_in, rpc_port } => {
            Wallet::complete(slate_in.clone(), *rpc_port).await?;
        }
    }

    Ok(())
}
