use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::{Arc, Mutex};

use crate::core::mempool::Mempool;
use crate::core::transaction::Transaction;
use super::dandelion::{DandelionRouter, TxState};

pub struct P2pServer {
    router: DandelionRouter,
    mempool: Arc<Mutex<Mempool>>,
}

impl P2pServer {
    pub fn new(mempool: Arc<Mutex<Mempool>>) -> Self {
        Self {
            router: DandelionRouter::new(0.10), // 10% fluff probability
            mempool,
        }
    }

    pub async fn start(&self, addr: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        println!("P2P Server listening on {}", addr);

        loop {
            let (mut socket, peer_addr) = listener.accept().await?;
            println!("New peer connected: {}", peer_addr);

            let mempool = Arc::clone(&self.mempool);
            let next_state = self.router.next_state();

            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => {
                        // Attempt to deserialize a transaction from the wire
                        if let Ok(tx) = bincode::deserialize::<Transaction>(&buf[0..n]) {
                            println!("Received valid transaction from {}", peer_addr);
                            
                            // Process Dandelion++ routing logic
                            match next_state {
                                TxState::Stem => {
                                    println!("Routing transaction in STEM phase (obfuscating origin)");
                                    // Forward to 1 peer in a real node
                                }
                                TxState::Fluff => {
                                    println!("Routing transaction in FLUFF phase (gossiping to all)");
                                    // Broadcast to all in a real node
                                }
                            }
                            
                            // Add to local mempool
                            let mut mp = mempool.lock().unwrap();
                            if mp.add_transaction(tx) {
                                println!("Transaction added to mempool");
                                let _ = socket.write_all(b"OK").await;
                            } else {
                                println!("Invalid transaction rejected");
                                let _ = socket.write_all(b"ERR: Invalid Tx").await;
                            }
                        }
                    },
                    _ => {}
                }
            });
        }
    }
}
