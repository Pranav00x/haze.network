use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use curve25519_dalek_ng::scalar::Scalar;
use bulletproofs::PedersenGens;

use super::mempool::Mempool;
use super::chain::ChainState;
use super::block::{Block, BlockHeader};
use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use crate::p2p::server::P2pServer;

pub struct Proposer {
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
    stake_key: Option<Scalar>,
    p2p_server: Mutex<Option<Arc<P2pServer>>>,
}

impl Proposer {
    pub fn new(
        mempool: Arc<Mutex<Mempool>>,
        chain: Arc<Mutex<ChainState>>,
        stake_key: Option<Scalar>,
    ) -> Self {
        Self {
            mempool,
            chain,
            stake_key,
            p2p_server: Mutex::new(None),
        }
    }

    /// Sets the P2P server instance to broadcast new blocks.
    pub fn set_p2p_server(&self, p2p_server: Arc<P2pServer>) {
        let mut server = self.p2p_server.lock().unwrap();
        *server = Some(p2p_server);
    }

    pub async fn start_proposing(&self) {
        if self.stake_key.is_none() {
            println!("Proposer started in passive validator mode (no staking key).");
            return;
        }
        let private_key = self.stake_key.unwrap();
        let gens = PedersenGens::default();
        let r_point = private_key * gens.B_blinding;

        println!("Staking proposer started. Monitoring slots...");

        loop {
            // Check slots every 5 seconds
            sleep(Duration::from_millis(5000)).await;

            let (next_height, prev_hash, my_validator) = {
                let c = self.chain.lock().unwrap();
                let next_height = c.current_height + 1;
                let prev_hash = c.last_block_hash;

                // Find our matching validator commitment in active validators
                let mut my_val = None;
                for val in &c.active_validators {
                    let derived_r = val.commitment.as_point() - Scalar::from(val.value) * gens.B;
                    if derived_r == r_point {
                        my_val = Some(val.clone());
                        break;
                    }
                }

                // If validator set is empty and we possess the genesis staking key (Scalar(42)), we are the genesis proposer
                if my_val.is_none() && c.active_validators.is_empty() && private_key == Scalar::from(42u64) {
                    my_val = Some(crate::core::chain::Validator {
                        commitment: Commitment::new(1_000_000, private_key),
                        value: 1_000_000,
                    });
                }

                (next_height, prev_hash, my_val)
            };

            if let Some(validator) = my_validator {
                // Determine if we are the chosen proposer for this slot
                let chosen_proposer = {
                    let c = self.chain.lock().unwrap();
                    c.select_proposer(next_height, prev_hash)
                };

                if chosen_proposer == validator.commitment {
                    // Check if there are transactions in the mempool
                    let tx_bundle = {
                        let mut mp = self.mempool.lock().unwrap();
                        mp.aggregate()
                    };

                    // Propose block if we have transactions
                    if let Some(tx) = tx_bundle {
                        println!("We are chosen proposer for block #{}! Proposing block with {} transactions...", next_height, tx.kernels.len());
                        
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        let mut header = BlockHeader {
                            height: next_height,
                            prev_hash,
                            total_kernel_offset: Scalar::zero(),
                            nonce: 0,
                            timestamp: now,
                            validator_commitment: validator.commitment,
                            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
                        };

                        // Sign the block header
                        let msg = header.hash();
                        header.validator_signature = Signature::sign(&msg, &private_key);

                        let block = Block {
                            header,
                            body: tx,
                        };

                        // Apply locally
                        let mut c = self.chain.lock().unwrap();
                        if c.apply_block(&block) {
                            println!("Block #{} successfully proposed and added to chain locally!", block.header.height);

                            // Save to disk
                            if let Err(e) = crate::core::storage::Storage::save_state(&c) {
                                println!("Warning: Failed to save chain state to disk: {}", e);
                            }

                            // Broadcast block
                            let server_opt = {
                                let s = self.p2p_server.lock().unwrap();
                                s.clone()
                            };
                            if let Some(p2p) = server_opt {
                                let p2p_clone = Arc::clone(&p2p);
                                let block_clone = block.clone();
                                tokio::spawn(async move {
                                    p2p_clone.broadcast_block(block_clone).await;
                                });
                            }
                        } else {
                            println!("Warning: Locally proposed block was rejected by local validation!");
                        }
                    }
                }
            }
        }
    }
}
