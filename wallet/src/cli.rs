use clap::{Parser, Subcommand};
use reqwest;
use curve25519_dalek_ng::scalar::Scalar;
use std::collections::HashSet;

use haze_crypto::pedersen::Commitment;
use super::keystore::Keystore;
use super::store::{WalletStore, OutputStatus, GENESIS_INDEX};
use super::planner::{self, PlanError};
use super::slate::{self, Slate, PendingSlate};

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
    /// Step 1 of paying a different wallet: builds a slate file to hand to the recipient
    Pay {
        #[arg(short, long)]
        amount: u64,

        #[arg(long, default_value = "slate.json")]
        slate_out: String,

        #[arg(short, long, default_value = "8332")]
        rpc_port: u16,
    },
    /// Step 2: responds to a slate file received from a sender, producing a response file to hand back
    Receive {
        #[arg(long)]
        slate_in: String,

        #[arg(long, default_value = "response.json")]
        slate_out: String,
    },
    /// Step 3: completes a payment using the recipient's response file and broadcasts it
    Complete {
        #[arg(long)]
        slate_in: String,

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

    fn read_slate(path: &str) -> std::io::Result<Slate> {
        let contents = std::fs::read_to_string(path)?;
        serde_json::from_str(&contents).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    fn write_slate(path: &str, slate: &Slate) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(slate)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Sender step 1: builds a slate paying a different wallet `amount`, writes
    /// it to `slate_out` for the recipient, and keeps the private half locally.
    pub async fn pay(amount: u64, slate_out: String, rpc_port: u16) -> std::io::Result<()> {
        let mut keystore = Keystore::load_or_create();
        let mut store = WalletStore::load_or_create();
        Self::reconcile_with_node(&mut store, rpc_port).await?;

        let (slate, pending) = match slate::create_slate(&mut keystore, &store, amount, FEE) {
            Ok(result) => result,
            Err(PlanError::InsufficientBalance { have, need }) => {
                println!(
                    "Error: insufficient confirmed balance. Have {}, need {} (amount {} + fee {}).",
                    have, need, amount, FEE
                );
                return Ok(());
            }
        };

        // Persist immediately, before anything else, so a crash never risks
        // reusing a blinding factor or losing the private half of this slate.
        keystore.save_to_file();
        pending.save();

        Self::write_slate(&slate_out, &slate)?;
        println!("Slate written to {}. Send this file to the recipient.", slate_out);
        println!("Once they respond, run `haze complete --slate-in <their response file>` to finish and broadcast.");

        Ok(())
    }

    /// Receiver step: fills in a slate received from a sender and writes the
    /// response for the sender to pick up. Uses this wallet's own keystore/store.
    pub async fn receive(slate_in: String, slate_out: String) -> std::io::Result<()> {
        let mut keystore = Keystore::load_or_create();
        let mut store = WalletStore::load_or_create();

        let incoming = Self::read_slate(&slate_in)?;
        let (response, owned_output) = slate::respond_to_slate(&mut keystore, &incoming);

        keystore.save_to_file();
        store.add_output(owned_output.index, owned_output.value, owned_output.commitment, OutputStatus::Pending);
        store.save();

        Self::write_slate(&slate_out, &response)?;
        println!("Received {} units. Response written to {}.", incoming.amount, slate_out);
        println!("Send this file back to the sender to complete the payment.");

        Ok(())
    }

    /// Sender step 2 (final): combines the local pending slate with the
    /// recipient's response, validates, and broadcasts the transaction.
    pub async fn complete(slate_in: String, rpc_port: u16) -> std::io::Result<()> {
        let pending = match PendingSlate::load() {
            Some(p) => p,
            None => {
                println!("Error: no pending slate found. Run `haze pay` first.");
                return Ok(());
            }
        };

        let response = Self::read_slate(&slate_in)?;

        let transaction = match slate::finalize_slate(&pending, &response) {
            Ok(tx) => tx,
            Err(_) => {
                println!("Error: the response slate is incomplete - has the recipient run `haze receive` on it?");
                return Ok(());
            }
        };

        println!("Transaction constructed! Validating locally...");
        if !transaction.validate() {
            println!("Error: Constructed transaction failed local validation!");
            return Ok(());
        }

        println!("Submitting to http://127.0.0.1:{}/v1/transactions via JSON-RPC...", rpc_port);

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/v1/transactions", rpc_port);
        match client.post(&url)
            .json(&transaction)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    println!("Transaction successfully broadcasted to the network!");

                    let mut store = WalletStore::load_or_create();
                    for commitment in &pending.spent_commitments {
                        store.mark_spent(commitment);
                    }
                    if let Some(change) = &pending.change {
                        store.add_output(change.index, change.value, change.output.commitment, OutputStatus::Pending);
                    }
                    store.save();
                    PendingSlate::delete();
                } else {
                    let err = resp.text().await.unwrap_or_default();
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
        println!("Registering validator stake: value={}...", value);

        let blinding_scalar = Scalar::from(blinding);
        let commitment = Commitment::new(value, blinding_scalar);
        let msg = haze_chain::chain::stake_registration_message(&commitment, value);
        let proof = haze_crypto::schnorr::Signature::sign(&msg, &blinding_scalar);

        #[derive(serde::Serialize)]
        struct StakePayload {
            commitment: Commitment,
            value: u64,
            proof: haze_crypto::schnorr::Signature,
        }

        let payload = StakePayload {
            commitment,
            value,
            proof,
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
