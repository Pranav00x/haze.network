use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

use super::mempool::Mempool;
use super::chain::ChainState;
use super::block::{Block, BlockHeader};

pub struct Miner {
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
}

impl Miner {
    pub fn new(mempool: Arc<Mutex<Mempool>>, chain: Arc<Mutex<ChainState>>) -> Self {
        Self { mempool, chain }
    }

    pub async fn start_mining(&self) {
        println!("Miner started. Waiting for transactions...");
        
        loop {
            // Check for transactions
            let tx_bundle = {
                let mut mp = self.mempool.lock().unwrap();
                mp.aggregate()
            };

            if let Some(tx) = tx_bundle {
                println!("Mining new block with {} inputs and {} outputs...", tx.inputs.len(), tx.outputs.len());
                
                // Get chain state for header info
                let (height, prev_hash) = {
                    let c = self.chain.lock().unwrap();
                    (c.current_height + 1, c.last_block_hash)
                };

                // Simple PoW
                let mut header = BlockHeader {
                    height,
                    prev_hash,
                    total_kernel_offset: curve25519_dalek::scalar::Scalar::ZERO, // Simplified for now
                    nonce: 0,
                };

                // We need 2 leading zero bytes in hash for this prototype (difficulty)
                loop {
                    let hash = header.hash();
                    if hash[0] == 0 && hash[1] == 0 {
                        println!("Block mined! Nonce: {}", header.nonce);
                        break;
                    }
                    header.nonce += 1;
                    
                    // Yield occasionally so we don't block the async runtime completely
                    if header.nonce % 10_000 == 0 {
                        tokio::task::yield_now().await;
                    }
                }

                let block = Block {
                    header,
                    body: tx,
                };

                // Apply to chain
                let mut c = self.chain.lock().unwrap();
                if c.apply_block(&block) {
                    println!("Block #{} successfully added to chain!", block.header.height);
                } else {
                    println!("Mined block was invalid.");
                    // In a real node, we would return txs to mempool
                }
            } else {
                // Wait for txs
                sleep(Duration::from_millis(2000)).await;
            }
        }
    }
}
