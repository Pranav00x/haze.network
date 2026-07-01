use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex as TokioMutex;
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use rand::Rng;

use crate::core::mempool::Mempool;
use crate::core::chain::{ChainState, ApplyResult};
use crate::core::block::Block;
use crate::core::storage::Storage;
use super::dandelion::{DandelionRouter, TxState, compute_tx_id};
use super::message::P2pMessage;

/// Maximum number of blocks sent per GetBlocks/BlocksBatch round during chain sync.
const SYNC_BATCH_SIZE: usize = 256;

/// Maximum allowed size (in bytes) for a single length-prefixed P2P message.
/// Guards against a peer claiming an oversized length and forcing a huge allocation.
const MAX_MESSAGE_SIZE: usize = 32 * 1024 * 1024;

/// Maximum number of simultaneous outbound+inbound connections a node will maintain.
/// Bounds automatic peer-discovery dialing so a node doesn't try to connect to
/// every address it ever hears about.
const MAX_PEERS: usize = 8;

/// Maximum number of addresses returned in a single PeersList response.
const MAX_PEERS_SHARED: usize = 50;

pub struct PeerManager {
    peers: Mutex<HashMap<String, Arc<TokioMutex<OwnedWriteHalf>>>>,
    /// Address book of peers' real, dialable listen addresses (learned via
    /// Handshake/PeersList) - distinct from `peers`, which is keyed by whatever
    /// address identifies the live connection (the inbound side's ephemeral
    /// remote port isn't dialable by anyone else).
    known_peers: Mutex<HashSet<String>>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: Mutex::new(HashMap::new()),
            known_peers: Mutex::new(HashSet::new()),
        }
    }

    /// Registers a peer's write half and returns the shared handle for it.
    pub fn add_peer(&self, addr: String, write_half: OwnedWriteHalf) -> Arc<TokioMutex<OwnedWriteHalf>> {
        let handle = Arc::new(TokioMutex::new(write_half));
        let mut peers = self.peers.lock().unwrap();
        peers.insert(addr, Arc::clone(&handle));
        handle
    }

    pub fn remove_peer(&self, addr: &str) {
        let mut peers = self.peers.lock().unwrap();
        peers.remove(addr);
    }

    pub fn is_connected(&self, addr: &str) -> bool {
        self.peers.lock().unwrap().contains_key(addr)
    }

    pub fn connection_count(&self) -> usize {
        self.peers.lock().unwrap().len()
    }

    /// Records a peer's real listen address in the address book.
    pub fn add_known_peer(&self, addr: String) {
        self.known_peers.lock().unwrap().insert(addr);
    }

    /// A bounded snapshot of known peer addresses, suitable for sharing via PeersList.
    pub fn known_peers_snapshot(&self) -> Vec<String> {
        self.known_peers.lock().unwrap().iter().take(MAX_PEERS_SHARED).cloned().collect()
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

        if let Some(peer_write) = peer {
            let mut peer_lock = peer_write.lock().await;
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
    storage: Arc<Storage>,
    pub peer_manager: Arc<PeerManager>,
}

impl P2pServer {
    pub fn new(mempool: Arc<Mutex<Mempool>>, chain: Arc<Mutex<ChainState>>, storage: Arc<Storage>) -> Self {
        Self {
            router: Arc::new(DandelionRouter::new(0.20)), // 20% fluff probability
            mempool,
            chain,
            storage,
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

        let own_listen_addr = addr.to_string();

        // Connect to seed peers outbound
        for peer in seed_peers {
            let pm = Arc::clone(&self.peer_manager);
            let mp = Arc::clone(&self.mempool);
            let c = Arc::clone(&self.chain);
            let st = Arc::clone(&self.storage);
            let r = Arc::clone(&self.router);
            let own_listen_addr = own_listen_addr.clone();

            tokio::spawn(async move {
                connect_to_peer(peer, pm, mp, c, st, r, own_listen_addr).await;
            });
        }

        // Accept inbound connections loop
        let pm = Arc::clone(&self.peer_manager);
        let mp = Arc::clone(&self.mempool);
        let c = Arc::clone(&self.chain);
        let st = Arc::clone(&self.storage);
        let r = Arc::clone(&self.router);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            println!("P2P: Inbound peer connected: {}", peer_addr);

            let pm_clone = Arc::clone(&pm);
            let mp_clone = Arc::clone(&mp);
            let c_clone = Arc::clone(&c);
            let st_clone = Arc::clone(&st);
            let r_clone = Arc::clone(&r);
            let peer_str = peer_addr.to_string();
            let own_listen_addr = own_listen_addr.clone();

            tokio::spawn(async move {
                let (read_half, write_half) = stream.into_split();
                let write_handle = pm_clone.add_peer(peer_str.clone(), write_half);
                handle_peer_connection(read_half, write_handle, peer_str, pm_clone, mp_clone, c_clone, st_clone, r_clone, own_listen_addr).await;
            });
        }
    }
}

/// Dials a peer outbound, performs the Handshake/ChainInfo/GetPeers exchange, and
/// hands the connection off to handle_peer_connection. Used both for the initial
/// --peers seed list and for peers learned via PeersList (peer discovery).
/// Returns a boxed future rather than being declared `async fn` so its return type
/// is a fixed, already-Send concrete type instead of an opaque one inferred from
/// the body. connect_to_peer and handle_peer_connection are mutually recursive
/// (peer discovery reconnects via connect_to_peer from inside
/// handle_peer_connection's PeersList arm): without this, rustc's auto-trait
/// inference for the opaque future types goes in a cycle and can't resolve.
fn connect_to_peer(
    peer_addr: String,
    pm: Arc<PeerManager>,
    mp: Arc<Mutex<Mempool>>,
    c: Arc<Mutex<ChainState>>,
    st: Arc<Storage>,
    r: Arc<DandelionRouter>,
    own_listen_addr: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(async move {
        println!("P2P: Connecting outbound to peer {}...", peer_addr);
        match TcpStream::connect(&peer_addr).await {
            Ok(mut stream) => {
                println!("P2P: Connected outbound to peer {}!", peer_addr);
                pm.add_known_peer(peer_addr.clone());

                // Send Handshake
                if write_msg(&mut stream, &P2pMessage::Handshake { listen_addr: own_listen_addr.clone() }).await.is_ok() {
                    // Also send our chain info so the peer can sync from us if they're behind
                    let (our_height, our_tip) = {
                        let cs = c.lock().unwrap();
                        (cs.current_height, cs.last_block_hash)
                    };
                    let _ = write_msg(&mut stream, &P2pMessage::ChainInfo { height: our_height, tip_hash: our_tip }).await;
                    let _ = write_msg(&mut stream, &P2pMessage::GetPeers).await;

                    let (read_half, write_half) = stream.into_split();
                    let write_handle = pm.add_peer(peer_addr.clone(), write_half);
                    handle_peer_connection(read_half, write_handle, peer_addr, pm, mp, c, st, r, own_listen_addr).await;
                }
            }
            Err(e) => {
                println!("P2P: Failed to connect outbound to peer {}: {}", peer_addr, e);
            }
        }
    })
}

/// Persists an ApplyResult's deltas to storage, logging (but not failing on) any error.
fn persist_apply_result(storage: &Storage, result: &ApplyResult) {
    match result {
        ApplyResult::Linear(delta) => {
            if let Err(e) = storage.persist_applied(delta) {
                println!("Warning: Failed to persist applied block: {}", e);
            }
        }
        ApplyResult::Reorg { rollbacks, applies } => {
            for rollback in rollbacks {
                if let Err(e) = storage.persist_rollback(rollback) {
                    println!("Warning: Failed to persist rollback: {}", e);
                }
            }
            for delta in applies {
                if let Err(e) = storage.persist_applied(delta) {
                    println!("Warning: Failed to persist applied block: {}", e);
                }
            }
        }
        ApplyResult::Rejected => {}
    }
}

async fn write_msg<W: AsyncWrite + Unpin>(stream: &mut W, msg: &P2pMessage) -> std::io::Result<()> {
    let bytes = bincode::serialize(msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let len = bytes.len() as u32;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn handle_peer_connection(
    mut read_half: OwnedReadHalf,
    write_half: Arc<TokioMutex<OwnedWriteHalf>>,
    peer_addr: String,
    pm: Arc<PeerManager>,
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
    storage: Arc<Storage>,
    router: Arc<DandelionRouter>,
    own_listen_addr: String,
) {
    let mut len_bytes = [0u8; 4];
    loop {
        match read_half.read_exact(&mut len_bytes).await {
            Ok(_) => {
                let len = u32::from_le_bytes(len_bytes) as usize;
                if len > MAX_MESSAGE_SIZE {
                    println!("P2P: Peer {} sent oversized message ({} bytes), disconnecting", peer_addr, len);
                    break;
                }
                let mut buf = vec![0u8; len];
                if read_half.read_exact(&mut buf).await.is_err() {
                    break;
                }

                if let Ok(msg) = bincode::deserialize::<P2pMessage>(&buf) {
                    match msg {
                        P2pMessage::Handshake { listen_addr } => {
                            println!("P2P: Handshake received from {} (listening on {})", peer_addr, listen_addr);
                            pm.add_known_peer(listen_addr);
                            let (our_height, our_tip) = {
                                let cs = chain.lock().unwrap();
                                (cs.current_height, cs.last_block_hash)
                            };
                            let mut w = write_half.lock().await;
                            let _ = write_msg(&mut *w, &P2pMessage::ChainInfo { height: our_height, tip_hash: our_tip }).await;
                            let _ = write_msg(&mut *w, &P2pMessage::GetPeers).await;
                        }
                        P2pMessage::Ping => {
                            let mut w = write_half.lock().await;
                            let _ = write_msg(&mut *w, &P2pMessage::Pong).await;
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
                            let result = {
                                let mut c = chain.lock().unwrap();
                                c.apply_block(&block)
                            };

                            if result.is_applied() {
                                println!("P2P: Applied block #{} successfully to chain state!", block.header.height);
                                persist_apply_result(&storage, &result);
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
                                let ok = c.register_validator(commitment, value, blinding);
                                if ok {
                                    if let Err(e) = storage.persist_active_validators(&c.active_validators) {
                                        println!("Warning: Failed to persist validator registration: {}", e);
                                    }
                                }
                                ok
                            };
                            if registered {
                                println!("P2P: Validator registered and propagated to peers.");
                                pm.broadcast(&P2pMessage::RegisterValidator { commitment, value, blinding }).await;
                            }
                        }
                        P2pMessage::ChainInfo { height, tip_hash } => {
                            let our_height = { chain.lock().unwrap().current_height };
                            println!("P2P: Peer {} reports chain height {} (tip {:?}, ours: {})", peer_addr, height, tip_hash, our_height);
                            if height > our_height {
                                let mut w = write_half.lock().await;
                                let _ = write_msg(&mut *w, &P2pMessage::GetBlocks { from_height: our_height + 1 }).await;
                            }
                        }
                        P2pMessage::GetBlocks { from_height } => {
                            let (blocks, has_more) = {
                                let cs = chain.lock().unwrap();
                                cs.get_blocks_from(from_height, SYNC_BATCH_SIZE)
                            };
                            println!("P2P: Sending {} blocks (from height {}) to {}", blocks.len(), from_height, peer_addr);
                            let mut w = write_half.lock().await;
                            let _ = write_msg(&mut *w, &P2pMessage::BlocksBatch { blocks, has_more }).await;
                        }
                        P2pMessage::BlocksBatch { blocks, has_more } => {
                            println!("P2P: Received batch of {} blocks from {} (has_more: {})", blocks.len(), peer_addr, has_more);
                            let batch_len = blocks.len();
                            let mut applied_count = 0;
                            for block in &blocks {
                                let result = {
                                    let mut c = chain.lock().unwrap();
                                    c.apply_block(block)
                                };
                                if result.is_applied() {
                                    applied_count += 1;
                                    persist_apply_result(&storage, &result);
                                    let mut mp = mempool.lock().unwrap();
                                    mp.clear_spent(&block.body);
                                } else {
                                    println!("P2P: Sync block #{} failed to apply, stopping sync from {}", block.header.height, peer_addr);
                                    break;
                                }
                            }
                            println!("P2P: Synced {} / {} blocks from {}", applied_count, batch_len, peer_addr);

                            if has_more && applied_count == batch_len {
                                let next_from = { chain.lock().unwrap().current_height + 1 };
                                let mut w = write_half.lock().await;
                                let _ = write_msg(&mut *w, &P2pMessage::GetBlocks { from_height: next_from }).await;
                            }
                        }
                        P2pMessage::GetPeers => {
                            let peers_list = pm.known_peers_snapshot();
                            let mut w = write_half.lock().await;
                            let _ = write_msg(&mut *w, &P2pMessage::PeersList(peers_list)).await;
                        }
                        P2pMessage::PeersList(addrs) => {
                            for candidate in addrs {
                                if candidate == own_listen_addr || pm.is_connected(&candidate) {
                                    continue;
                                }
                                pm.add_known_peer(candidate.clone());
                                if pm.connection_count() >= MAX_PEERS {
                                    continue;
                                }
                                println!("P2P: Discovered new peer {} via {}, connecting...", candidate, peer_addr);
                                let pm2 = Arc::clone(&pm);
                                let mp2 = Arc::clone(&mempool);
                                let c2 = Arc::clone(&chain);
                                let st2 = Arc::clone(&storage);
                                let r2 = Arc::clone(&router);
                                let own_listen_addr2 = own_listen_addr.clone();
                                tokio::spawn(async move {
                                    connect_to_peer(candidate, pm2, mp2, c2, st2, r2, own_listen_addr2).await;
                                });
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
