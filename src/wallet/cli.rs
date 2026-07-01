use clap::{Parser, Subcommand};
use reqwest;
use curve25519_dalek_ng::scalar::Scalar;
use std::collections::HashSet;

use crate::crypto::pedersen::Commitment;
use super::keystore::Keystore;
use super::store::{WalletStore, OutputStatus, GENESIS_INDEX};
use super::planner::{self, PlanError};

const FEE: u64 = 5;

#[derive(Parser)]
#[command(name = "haze")]
#[command(about = "Haze Mimblewimble L1 Node & Wallet", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Starts the Haze blockchain node
    Node {
        #[arg(short, long, default_value = "127.0.0.1:8333")]
        bind: String,

        #[arg(short, long)]
        peers: Option<String>,

        #[arg(short, long, default_value = "8332")]
        rpc_port: u16,

        #[arg(short, long)]
        stake_key: Option<String>,
    },
    /// Initializes the local wallet keystore
    Init {
        /// Seed the wallet ledger with the well-known devnet genesis output (1,000,000 haze, blinding=42)
        #[arg(long)]
        claim_genesis: bool,
    },
    /// Shows the wallet's confirmed and pending balance
    Balance {
        #[arg(short, long, default_value = "8332")]
        rpc_port: u16,
    },
    /// Sends (self-pays) an amount from the wallet's own confirmed UTXOs
    Send {
        #[arg(short, long)]
        amount: u64,

        #[arg(short, long, default_value = "8332")]
        rpc_port: u16,
    },
    /// Locks a UTXO as stake and registers as a validator
    Stake {
        #[arg(short, long)]
        value: u64,
        #[arg(short, long)]
        blinding: u64,
        #[arg(short, long, default_value = "8332")]
        rpc_port: u16,
    },
}

pub struct Wallet;

impl Wallet {
    /// Loads the keystore, optionally seeding the local ledger with the known genesis output.
    pub fn init(claim_genesis: bool) -> std::io::Result<()> {
        let _keystore = Keystore::load_or_create();
        let mut store = WalletStore::load_or_create();

        println!("Wallet keystore ready in wallet_data/");

        if claim_genesis {
            if store.has_index(GENESIS_INDEX) {
                println!("Genesis output already claimed by this wallet.");
            } else {
                let genesis_blinding = Scalar::from(42u64);
                let genesis_value = 1_000_000u64;
                let commitment = Commitment::new(genesis_value, genesis_blinding);
                store.add_output(GENESIS_INDEX, genesis_value, commitment, OutputStatus::Confirmed);
                store.save();
                println!("Claimed well-known devnet genesis output (value=1,000,000).");
                println!("Note: this is a shared, publicly-known devnet secret (blinding=42) - only one wallet instance should spend it.");
            }
        }

        Ok(())
    }

    /// Fetches the node's current UTXO commitment set and reconciles it against the local ledger.
    async fn reconcile_with_node(store: &mut WalletStore, rpc_port: u16) -> std::io::Result<()> {
        let url = format!("http://127.0.0.1:{}/v1/utxos", rpc_port);
        let client = reqwest::Client::new();
        match client.get(&url).send().await {
            Ok(response) => {
                match response.json::<Vec<Commitment>>().await {
                    Ok(utxos) => {
                        let set: HashSet<Commitment> = utxos.into_iter().collect();
                        store.reconcile(&set);
                        store.save();
                    }
                    Err(e) => {
                        println!("Warning: failed to parse node UTXO set response: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Warning: failed to reach node at {} to reconcile UTXOs: {}", url, e);
            }
        }
        Ok(())
    }

    pub async fn balance(rpc_port: u16) -> std::io::Result<()> {
        let mut store = WalletStore::load_or_create();
        Self::reconcile_with_node(&mut store, rpc_port).await?;

        println!("Confirmed balance: {}", store.balance());
        println!("Pending balance:   {}", store.pending_balance());

        Ok(())
    }

    pub async fn send(amount: u64, rpc_port: u16) -> std::io::Result<()> {
        let mut keystore = Keystore::load_or_create();
        let mut store = WalletStore::load_or_create();
        Self::reconcile_with_node(&mut store, rpc_port).await?;

        let plan = match planner::plan_send(&mut keystore, &store, amount, FEE) {
            Ok(plan) => plan,
            Err(PlanError::InsufficientBalance { have, need }) => {
                println!(
                    "Error: insufficient confirmed balance. Have {}, need {} (amount {} + fee {}).",
                    have, need, amount, FEE
                );
                return Ok(());
            }
        };

        // Persist the keystore's allocated indices immediately, before any network
        // I/O, so a crash never risks reusing a blinding factor.
        keystore.save_to_file();

        println!("Transaction constructed! Validating locally...");
        if !plan.transaction.validate() {
            println!("Error: Constructed transaction failed local validation!");
            return Ok(());
        }

        println!("Submitting to http://127.0.0.1:{}/v1/transactions via JSON-RPC...", rpc_port);

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/v1/transactions", rpc_port);
        match client.post(&url)
            .json(&plan.transaction)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    println!("Transaction successfully broadcasted to the network!");

                    // Update local ledger only on success.
                    for commitment in &plan.spent_commitments {
                        store.mark_spent(commitment);
                    }
                    let (dest_index, dest_commitment, dest_value) = plan.dest;
                    store.add_output(dest_index, dest_value, dest_commitment, OutputStatus::Pending);
                    if let Some((change_index, change_commitment, change_value)) = plan.change {
                        store.add_output(change_index, change_value, change_commitment, OutputStatus::Pending);
                    }
                    store.save();
                } else {
                    let err = response.text().await.unwrap_or_default();
                    println!("Node rejected transaction: {}", err);
                }
            }
            Err(e) => {
                println!("Failed to connect to the node. Is it running? Error: {}", e);
            }
        }

        Ok(())
    }

    pub async fn stake(value: u64, blinding: u64, rpc_port: u16) -> std::io::Result<()> {
        println!("Registering validator stake: value={}, blinding={}...", value, blinding);

        let blinding_scalar = Scalar::from(blinding);
        let commitment = Commitment::new(value, blinding_scalar);

        #[derive(serde::Serialize)]
        struct StakePayload {
            commitment: Commitment,
            value: u64,
            blinding: Scalar,
        }

        let payload = StakePayload {
            commitment,
            value,
            blinding: blinding_scalar,
        };

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/v1/stake", rpc_port);
        match client.post(&url)
            .json(&payload)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    println!("Stake successfully registered on node and propagated to peers!");
                } else {
                    let err = response.text().await.unwrap_or_default();
                    println!("Validator registration rejected: {}", err);
                }
            }
            Err(e) => {
                println!("Failed to connect to node API. Is the node running? Error: {}", e);
            }
        }

        Ok(())
    }
}
