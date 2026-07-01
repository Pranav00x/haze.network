use clap::{Parser, Subcommand};
use rand::{thread_rng, Rng};
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use curve25519_dalek_ng::scalar::Scalar;

use crate::crypto::pedersen::Commitment;
use crate::crypto::range_proof::RangeProof;
use crate::crypto::schnorr::Signature;
use crate::core::transaction::{Transaction, Input, Output, TxKernel};

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
    Node,
    /// Sends a dummy transaction to the local node
    Send {
        #[arg(short, long)]
        amount: u64,
    },
}

pub struct Wallet;

impl Wallet {
    fn random_scalar(rng: &mut impl Rng) -> Scalar {
        let mut bytes = [0u8; 64];
        rng.fill(&mut bytes);
        Scalar::from_bytes_mod_order_wide(&bytes)
    }

    pub async fn send_dummy_transaction(amount: u64) -> std::io::Result<()> {
        println!("Constructing a dummy Mimblewimble transaction for {} units...", amount);
        
        let mut rng = thread_rng();
        
        // 1. Create a fake input (In a real wallet, this comes from our UTXO set)
        let input_blinding = Self::random_scalar(&mut rng);
        let input_value = amount + 5; // We have exactly enough to pay amount + fee
        let input_commitment = Commitment::new(input_value, input_blinding);
        let input = Input {
            commitment: input_commitment,
        };

        // 2. Create the output
        let output_blinding = Self::random_scalar(&mut rng);
        let output_value = amount;
        let output_commitment = Commitment::new(output_value, output_blinding);
        
        println!("Generating Bulletproofs Range Proof (this takes a moment)...");
        let proof = RangeProof::prove(output_value, &output_blinding);
        
        let output = Output {
            commitment: output_commitment,
            proof,
        };

        // 3. Calculate Kernel Excess
        let fee = 5u64;
        // The transaction balance equation: sum(output) - sum(input) + fee = 0
        // Blinding equation: sum(output_blinding) - sum(input_blinding) = excess
        let excess_blinding = output_blinding - input_blinding;
        
        // The excess commitment commits to 0 value, with the excess blinding factor
        let excess_commitment = Commitment::new(0, excess_blinding);
        
        let signature = Signature::sign(&fee.to_le_bytes(), &excess_blinding);
        
        let kernel = TxKernel {
            excess: excess_commitment,
            fee,
            signature,
        };

        let tx = Transaction {
            inputs: vec![input],
            outputs: vec![output],
            kernels: vec![kernel],
        };
        
        println!("Transaction constructed! Validating locally...");
        if !tx.validate() {
            println!("Error: Constructed transaction failed local validation!");
            return Ok(());
        }

        println!("Serialization...");
        let encoded = bincode::serialize(&tx).unwrap();
        
        println!("Broadcasting to 127.0.0.1:8333...");
        if let Ok(mut stream) = TcpStream::connect("127.0.0.1:8333").await {
            stream.write_all(&encoded).await?;
            println!("Transaction successfully broadcasted to the network!");
        } else {
            println!("Failed to connect to the node. Is it running?");
        }

        Ok(())
    }
}
