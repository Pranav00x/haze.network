use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex as TokioMutex;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use rand::Rng;

use crate::core::mempool::Mempool;
use crate::core::chain::ChainState;
use crate::core::block::Block;
use super::dandelion::{DandelionRouter, TxState, compute_tx_id};
use super::message::P2pMessage;

pub struct PeerManager {
    peers: Mutex<HashMap<String, Arc<TokioMutex<TcpStream>>>>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_peer(&self, addr: String, stream: TcpStream) {
        let mut peers = self.peers.lock().unwrap();
        peers.insert(addr, Arc::new(TokioMutex::new(stream)));
    }

    pub fn remove_peer(&self, addr: &str) {
        let mut peers = self.peers.lock().unwrap();
        peers.remove(addr);
    }

    pub async fn broadcast(&self, msg: &P2pMessage) {
        let bytes = match bincode::serialize(msg) {
            Ok(b) => b,
            Err(_) => return,
        };
        let len = bytes.len() as u32;

        let peers = {
            let p = self.peers.lock().unwrap();
            p.values().cloned().collect::<Vec<_>>()
        };

        for peer in peers {
            let mut peer_lock = peer.lock().await;
            let _ = peer_lock.write_all(&len.to_le_bytes()).await;
            let _ = peer_lock.write_all(&bytes).await;
            let _ = peer_lock.flush().await;
        }
    }

    pub async fn send_to_random_peer(&self, msg: &P2pMessage) -> bool {
        let bytes = match bincode::serialize(msg) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let len = bytes.len() as u32;

        let peer = {
            let p = self.peers.lock().unwrap();
            if p.is_empty() {
                return false;
            }
            let keys: Vec<_> = p.keys().cloned().collect();
            let mut rng = rand::thread_rng();
            let random_key = &keys[rng.gen_range(0..keys.len())];
            p.get(random_key).cloned()
        };

        if let Some(peer_stream) = peer {
            let mut peer_lock = peer_stream.lock().await;
            if peer_lock.write_all(&len.to_le_bytes()).await.is_ok() &&
               peer_lock.write_all(&bytes).await.is_ok() &&
               peer_lock.flush().await.is_ok() {
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

pub struct P2pServer {
    router: Arc<DandelionRouter>,
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
    pub peer_manager: Arc<PeerManager>,
}

impl P2pServer {
    pub fn new(mempool: Arc<Mutex<Mempool>>, chain: Arc<Mutex<ChainState>>) -> Self {
        Self {
            router: Arc::new(DandelionRouter::new(0.20)), // 20% fluff probability
            mempool,
            chain,
            peer_manager: Arc::new(PeerManager::new()),
        }
    }

    pub async fn broadcast_block(&self, block: Block) {
        println!("P2P: Broadcasting newly proposed Block #{} to the network...", block.header.height);
        self.peer_manager.broadcast(&P2pMessage::NewBlock(block)).await;
    }

    pub async fn start(&self, addr: &str, seed_peers: Vec<String>) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        println!("P2P Server listening on {}", addr);

        // Connect to seed peers outbound
        for peer in seed_peers {
            let peer_addr = peer.clone();
            let pm = Arc::clone(&self.peer_manager);
            let mp = Arc::clone(&self.mempool);
            let c = Arc::clone(&self.chain);
            let r = Arc::clone(&self.router);
            let local_listen_addr = addr.to_string();

            tokio::spawn(async move {
                println!("P2P: Connecting outbound to seed peer {}...", peer_addr);
                match TcpStream::connect(&peer_addr).await {
                    Ok(mut stream) => {
                        println!("P2P: Connected outbound to peer {}!", peer_addr);
                        
                        // Send Handshake
                        if write_msg(&mut stream, &P2pMessage::Handshake { listen_addr: local_listen_addr }).await.is_ok() {
                            pm.add_peer(peer_addr.clone(), stream);
                            // Start connection handle loop
                            let active_stream = {
                                let p = pm.peers.lock().unwrap();
                                p.get(&peer_addr).cloned()
                            };
                            if let Some(s) = active_stream {
                                handle_peer_connection(s, peer_addr, pm, mp, c, r).await;
                            }
                        }
                    }
                    Err(e) => {
                        println!("P2P: Failed to connect outbound to seed peer {}: {}", peer_addr, e);
                    }
                }
            });
        }

        // Accept inbound connections loop
        let pm = Arc::clone(&self.peer_manager);
        let mp = Arc::clone(&self.mempool);
        let c = Arc::clone(&self.chain);
        let r = Arc::clone(&self.router);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            println!("P2P: Inbound peer connected: {}", peer_addr);

            let pm_clone = Arc::clone(&pm);
            let mp_clone = Arc::clone(&mp);
            let c_clone = Arc::clone(&c);
            let r_clone = Arc::clone(&r);
            let peer_str = peer_addr.to_string();

            tokio::spawn(async move {
                pm_clone.add_peer(peer_str.clone(), stream);
                let active_stream = {
                    let p = pm_clone.peers.lock().unwrap();
                    p.get(&peer_str).cloned()
                };
                if let Some(s) = active_stream {
                    handle_peer_connection(s, peer_str, pm_clone, mp_clone, c_clone, r_clone).await;
                }
            });
        }
    }
}

async fn write_msg(stream: &mut TcpStream, msg: &P2pMessage) -> std::io::Result<()> {
    let bytes = bincode::serialize(msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let len = bytes.len() as u32;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn handle_peer_connection(
    stream_arc: Arc<TokioMutex<TcpStream>>,
    peer_addr: String,
    pm: Arc<PeerManager>,
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
    router: Arc<DandelionRouter>,
) {
    let mut len_bytes = [0u8; 4];
    loop {
        let mut stream = stream_arc.lock().await;
        stream.readable().await.unwrap();
        match stream.read_exact(&mut len_bytes).await {
            Ok(_) => {
                let len = u32::from_le_bytes(len_bytes) as usize;
                let mut buf = vec![0u8; len];
                if stream.read_exact(&mut buf).await.is_err() {
                    break;
                }
                drop(stream); // release lock during processing

                if let Ok(msg) = bincode::deserialize::<P2pMessage>(&buf) {
                    match msg {
                        P2pMessage::Handshake { listen_addr } => {
                            println!("P2P: Handshake received from {} (listening on {})", peer_addr, listen_addr);
                        }
                        P2pMessage::Ping => {
                            let mut stream = stream_arc.lock().await;
                            let _ = write_msg(&mut *stream, &P2pMessage::Pong).await;
                        }
                        P2pMessage::Pong => {
                            // Alive confirmation
                        }
                        P2pMessage::StemTx(tx) => {
                            let tx_id = compute_tx_id(&tx);
                            println!("P2P: Received StemTx {:?} from {}", tx_id, peer_addr);

                            let already_fluffed = router.is_fluffed(tx_id);
                            if !already_fluffed {
                                // Decide next dandelion state
                                match router.next_state() {
                                    TxState::Stem => {
                                        println!("Dandelion++: Routing stem transaction {:?} to next stem hop", tx_id);
                                        // Forward to a random peer
                                        let forwarded = pm.send_to_random_peer(&P2pMessage::StemTx(tx.clone())).await;
                                        
                                        // Fallback timer: if we don't hear a fluff within 15s, self-fluff
                                        let pm_fallback = Arc::clone(&pm);
                                        let tx_fallback = tx.clone();
                                        router.register_stem_tx(tx_id, 15, move || {
                                            tokio::spawn(async move {
                                                println!("Dandelion++: Fallback fluff triggered for {:?}", tx_id);
                                                pm_fallback.broadcast(&P2pMessage::FluffTx(tx_fallback)).await;
                                            });
                                        });

                                        if !forwarded {
                                            // No peers available to stem, so fluff immediately
                                            println!("Dandelion++: No peers available to stem routing. Fluffing immediately!");
                                            router.mark_fluffed(tx_id);
                                            {
                                                let mut mp = mempool.lock().unwrap();
                                                mp.add_transaction(tx.clone());
                                            }
                                            pm.broadcast(&P2pMessage::FluffTx(tx)).await;
                                        }
                                    }
                                    TxState::Fluff => {
                                        println!("Dandelion++: Fluffing stem transaction {:?} (broadcasting)", tx_id);
                                        router.mark_fluffed(tx_id);
                                        let added = {
                                            let mut mp = mempool.lock().unwrap();
                                            mp.add_transaction(tx.clone())
                                        };
                                        if added {
                                            pm.broadcast(&P2pMessage::FluffTx(tx)).await;
                                        }
                                    }
                                }
                            }
                        }
                        P2pMessage::FluffTx(tx) => {
                            let tx_id = compute_tx_id(&tx);
                            if !router.is_fluffed(tx_id) {
                                println!("P2P: Received FluffTx {:?} from {}. Processing...", tx_id, peer_addr);
                                router.mark_fluffed(tx_id);
                                
                                let added = {
                                    let mut mp = mempool.lock().unwrap();
                                    mp.add_transaction(tx.clone())
                                };
                                if added {
                                    // Gossip to all other peers
                                    pm.broadcast(&P2pMessage::FluffTx(tx)).await;
                                }
                            }
                        }
                        P2pMessage::NewBlock(block) => {
                            println!("P2P: Received NewBlock #{} from {}", block.header.height, peer_addr);
                            let applied = {
                                let mut c = chain.lock().unwrap();
                                c.apply_block(&block)
                            };

                            if applied {
                                println!("P2P: Applied block #{} successfully to chain state!", block.header.height);
                                // Clear spent mempool transactions
                                {
                                    let mut mp = mempool.lock().unwrap();
                                    mp.clear_spent(&block.body);
                                }
                                // Propagate block
                                pm.broadcast(&P2pMessage::NewBlock(block)).await;
                            }
                        }
                        P2pMessage::RegisterValidator { commitment, value, blinding } => {
                            println!("P2P: Received RegisterValidator for commitment {:?}", commitment);
                            let registered = {
                                let mut c = chain.lock().unwrap();
                                c.register_validator(commitment, value, blinding)
                            };
                            if registered {
                                println!("P2P: Validator registered and propagated to peers.");
                                pm.broadcast(&P2pMessage::RegisterValidator { commitment, value, blinding }).await;
                            }
                        }
                    }
                }
            }
            Err(_) => {
                break;
            }
        }
    }
    println!("P2P: Connection with {} closed.", peer_addr);
    pm.remove_peer(&peer_addr);
}
