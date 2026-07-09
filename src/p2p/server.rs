use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex as TokioMutex;
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use rand::Rng;
use futures_util::StreamExt;

use crate::core::mempool::Mempool;
use crate::core::chain::{ChainState, ApplyResult};
use crate::core::block::Block;
use crate::core::transaction::{Transaction, TxKernel};
use crate::core::storage::Storage;
use crate::core::marketplace::MarketplaceState;
use crate::core::allowlist::AllowlistState;
use crate::crypto::pedersen::Commitment;
use super::dandelion::{DandelionRouter, TxState, compute_tx_id};
use super::message::P2pMessage;
use super::transport::{self, PeerReader, PeerWriter, MAX_MESSAGE_SIZE};

/// Maximum number of blocks sent per GetBlocks/BlocksBatch round during chain sync.
const SYNC_BATCH_SIZE: usize = 256;

/// Maximum number of simultaneous outbound+inbound connections a node will maintain.
/// Bounds automatic peer-discovery dialing so a node doesn't try to connect to
/// every address it ever hears about.
const MAX_PEERS: usize = 8;

/// Maximum number of addresses returned in a single PeersList response.
const MAX_PEERS_SHARED: usize = 50;

pub struct PeerManager {
    peers: Mutex<HashMap<String, Arc<TokioMutex<PeerWriter>>>>,
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

    /// Registers a peer's writer (whichever transport) and returns the shared handle for it.
    pub fn add_peer(&self, addr: String, writer: PeerWriter) -> Arc<TokioMutex<PeerWriter>> {
        let handle = Arc::new(TokioMutex::new(writer));
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
        let peers = {
            let p = self.peers.lock().unwrap();
            p.values().cloned().collect::<Vec<_>>()
        };

        for peer in peers {
            let mut peer_lock = peer.lock().await;
            let _ = transport::write_message(&mut peer_lock, msg).await;
        }
    }

    pub async fn send_to_random_peer(&self, msg: &P2pMessage) -> bool {
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
            transport::write_message(&mut peer_lock, msg).await.is_ok()
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
    marketplace: Arc<MarketplaceState>,
    allowlist: Arc<AllowlistState>,
    pub peer_manager: Arc<PeerManager>,
    /// This node's own configured --bind address, mirrored here (in addition
    /// to being threaded through start()'s TCP call chain as before) so
    /// handle_inbound_ws - reached via the API server, outside start()'s
    /// scope - has access to it too.
    own_listen_addr: Mutex<String>,
}

impl P2pServer {
    pub fn new(mempool: Arc<Mutex<Mempool>>, chain: Arc<Mutex<ChainState>>, storage: Arc<Storage>, marketplace: Arc<MarketplaceState>, allowlist: Arc<AllowlistState>) -> Self {
        Self {
            router: Arc::new(DandelionRouter::new(0.20)), // 20% fluff probability
            mempool,
            chain,
            storage,
            marketplace,
            allowlist,
            peer_manager: Arc::new(PeerManager::new()),
            own_listen_addr: Mutex::new(String::new()),
        }
    }

    pub async fn broadcast_block(&self, block: Block) {
        println!("P2P: Broadcasting newly proposed Block #{} to the network...", block.header.height);
        self.peer_manager.broadcast(&P2pMessage::NewBlock(block)).await;
    }

    /// Entry point for a transaction that just originated on this node (via
    /// POST /v1/transactions) into Dandelion++ routing - see
    /// dispatch_dandelion_tx for why this can't just be a flat broadcast.
    pub async fn propagate_new_transaction(&self, tx: Transaction, already_in_mempool: bool) {
        dispatch_dandelion_tx(tx, already_in_mempool, &self.mempool, &self.peer_manager, &self.router).await;
    }

    /// Hands an inbound WebSocket connection (accepted via warp::ws() in
    /// api/server.rs - see src/p2p/transport.rs for why this transport
    /// exists) into the same peer-handling machinery as a raw TCP inbound
    /// connection, so WS and TCP peers gossip/sync with each other
    /// transparently through the shared PeerManager.
    pub async fn handle_inbound_ws(self: Arc<Self>, ws: warp::ws::WebSocket, peer_label: String) {
        println!("P2P: Inbound WebSocket peer connected: {}", peer_label);
        let (ws_write, ws_read) = ws.split();
        let write_handle = self.peer_manager.add_peer(peer_label.clone(), PeerWriter::WsServer(ws_write));
        let own_listen_addr = self.own_listen_addr.lock().unwrap().clone();
        handle_peer_connection(
            PeerReader::WsServer(ws_read),
            write_handle,
            peer_label,
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.mempool),
            Arc::clone(&self.chain),
            Arc::clone(&self.storage),
            Arc::clone(&self.marketplace),
            Arc::clone(&self.allowlist),
            Arc::clone(&self.router),
            own_listen_addr,
        ).await;
    }

    pub async fn start(&self, addr: &str, seed_peers: Vec<String>) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        println!("P2P Server listening on {}", addr);

        let own_listen_addr = addr.to_string();
        *self.own_listen_addr.lock().unwrap() = own_listen_addr.clone();

        // Connect to seed peers outbound
        for peer in seed_peers {
            let pm = Arc::clone(&self.peer_manager);
            let mp = Arc::clone(&self.mempool);
            let c = Arc::clone(&self.chain);
            let st = Arc::clone(&self.storage);
            let mkt = Arc::clone(&self.marketplace);
            let al = Arc::clone(&self.allowlist);
            let r = Arc::clone(&self.router);
            let own_listen_addr = own_listen_addr.clone();

            tokio::spawn(async move {
                connect_to_peer(peer, pm, mp, c, st, mkt, al, r, own_listen_addr).await;
            });
        }

        // Accept inbound connections loop
        let pm = Arc::clone(&self.peer_manager);
        let mp = Arc::clone(&self.mempool);
        let c = Arc::clone(&self.chain);
        let st = Arc::clone(&self.storage);
        let mkt = Arc::clone(&self.marketplace);
        let al = Arc::clone(&self.allowlist);
        let r = Arc::clone(&self.router);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            println!("P2P: Inbound peer connected: {}", peer_addr);

            let pm_clone = Arc::clone(&pm);
            let mp_clone = Arc::clone(&mp);
            let c_clone = Arc::clone(&c);
            let st_clone = Arc::clone(&st);
            let mkt_clone = Arc::clone(&mkt);
            let al_clone = Arc::clone(&al);
            let r_clone = Arc::clone(&r);
            let peer_str = peer_addr.to_string();
            let own_listen_addr = own_listen_addr.clone();

            tokio::spawn(async move {
                let (read_half, write_half) = stream.into_split();
                let write_handle = pm_clone.add_peer(peer_str.clone(), PeerWriter::Tcp(write_half));
                handle_peer_connection(PeerReader::Tcp(read_half), write_handle, peer_str, pm_clone, mp_clone, c_clone, st_clone, mkt_clone, al_clone, r_clone, own_listen_addr).await;
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
    mkt: Arc<MarketplaceState>,
    al: Arc<AllowlistState>,
    r: Arc<DandelionRouter>,
    own_listen_addr: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(async move {
        if peer_addr.starts_with("ws://") || peer_addr.starts_with("wss://") {
            connect_to_ws_peer(peer_addr, pm, mp, c, st, mkt, al, r, own_listen_addr).await;
            return;
        }

        println!("P2P: Connecting outbound to peer {}...", peer_addr);
        match TcpStream::connect(&peer_addr).await {
            Ok(stream) => {
                println!("P2P: Connected outbound to peer {}!", peer_addr);
                pm.add_known_peer(peer_addr.clone());

                let (read_half, write_half) = stream.into_split();
                let mut writer = PeerWriter::Tcp(write_half);

                // Send Handshake
                if transport::write_message(&mut writer, &P2pMessage::Handshake { listen_addr: own_listen_addr.clone() }).await.is_ok() {
                    // Also send our chain info so the peer can sync from us if they're behind
                    let (our_height, our_tip) = {
                        let cs = c.lock().unwrap();
                        (cs.current_height, cs.last_block_hash)
                    };
                    let _ = transport::write_message(&mut writer, &P2pMessage::ChainInfo { height: our_height, tip_hash: our_tip }).await;
                    let _ = transport::write_message(&mut writer, &P2pMessage::GetPeers).await;

                    let write_handle = pm.add_peer(peer_addr.clone(), writer);
                    handle_peer_connection(PeerReader::Tcp(read_half), write_handle, peer_addr, pm, mp, c, st, mkt, al, r, own_listen_addr).await;
                }
            }
            Err(e) => {
                println!("P2P: Failed to connect outbound to peer {}: {}", peer_addr, e);
            }
        }
    })
}

/// Dials a peer over WebSocket instead of raw TCP - used when a --peers
/// entry (either from the initial seed list or learned via PeersList) is a
/// ws(s):// URL rather than a plain host:port. See src/p2p/transport.rs for
/// why this transport exists.
async fn connect_to_ws_peer(
    peer_addr: String,
    pm: Arc<PeerManager>,
    mp: Arc<Mutex<Mempool>>,
    c: Arc<Mutex<ChainState>>,
    st: Arc<Storage>,
    mkt: Arc<MarketplaceState>,
    al: Arc<AllowlistState>,
    r: Arc<DandelionRouter>,
    own_listen_addr: String,
) {
    println!("P2P: Connecting outbound via WebSocket to peer {}...", peer_addr);
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
        max_message_size: Some(MAX_MESSAGE_SIZE),
        max_frame_size: Some(MAX_MESSAGE_SIZE),
        ..Default::default()
    };
    match tokio_tungstenite::connect_async_with_config(&peer_addr, Some(config), false).await {
        Ok((ws_stream, _response)) => {
            println!("P2P: Connected outbound via WebSocket to {}!", peer_addr);
            pm.add_known_peer(peer_addr.clone());

            let (ws_write, ws_read) = ws_stream.split();
            let mut writer = PeerWriter::WsClient(ws_write);

            if transport::write_message(&mut writer, &P2pMessage::Handshake { listen_addr: own_listen_addr.clone() }).await.is_ok() {
                let (our_height, our_tip) = {
                    let cs = c.lock().unwrap();
                    (cs.current_height, cs.last_block_hash)
                };
                let _ = transport::write_message(&mut writer, &P2pMessage::ChainInfo { height: our_height, tip_hash: our_tip }).await;
                let _ = transport::write_message(&mut writer, &P2pMessage::GetPeers).await;

                let write_handle = pm.add_peer(peer_addr.clone(), writer);
                handle_peer_connection(PeerReader::WsClient(ws_read), write_handle, peer_addr, pm, mp, c, st, mkt, al, r, own_listen_addr).await;
            }
        }
        Err(e) => {
            println!("P2P: Failed to connect outbound via WebSocket to {}: {}", peer_addr, e);
        }
    }
}

/// Routes a transaction through Dandelion++ stem/fluff, exactly the same
/// way regardless of whether it was just received from a peer as a
/// StemTx (already_in_mempool: false - a mid-path relay hop doesn't add it
/// to its own mempool until it actually fluffs) or just originated locally
/// via POST /v1/transactions (already_in_mempool: true - the HTTP handler
/// already added it to the mempool before this runs, so the wallet gets an
/// immediate accept/reject response). A brand-new local transaction must
/// enter the stem phase the same way any relayed hop does - broadcasting it
/// immediately would defeat Dandelion's entire purpose of making the
/// originating node indistinguishable from a relay.
async fn dispatch_dandelion_tx(
    tx: Transaction,
    already_in_mempool: bool,
    mempool: &Arc<Mutex<Mempool>>,
    pm: &Arc<PeerManager>,
    router: &Arc<DandelionRouter>,
) {
    let tx_id = compute_tx_id(&tx);
    if router.is_fluffed(tx_id) {
        return;
    }
    match router.next_state() {
        TxState::Stem => {
            println!("Dandelion++: Routing stem transaction {:?} to next stem hop", tx_id);
            let forwarded = pm.send_to_random_peer(&P2pMessage::StemTx(tx.clone())).await;

            // Fallback timer: if we don't hear a fluff within 15s, self-fluff
            let pm_fallback = Arc::clone(pm);
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
                if !already_in_mempool {
                    let mut mp = mempool.lock().unwrap();
                    mp.add_transaction(tx.clone());
                }
                pm.broadcast(&P2pMessage::FluffTx(tx)).await;
            }
        }
        TxState::Fluff => {
            println!("Dandelion++: Fluffing stem transaction {:?} (broadcasting)", tx_id);
            router.mark_fluffed(tx_id);
            let added = if already_in_mempool {
                true
            } else {
                let mut mp = mempool.lock().unwrap();
                mp.add_transaction(tx.clone())
            };
            if added {
                pm.broadcast(&P2pMessage::FluffTx(tx)).await;
            }
        }
    }
}

/// Folds one block's kernels (main body + name/mint fee-payments) into a
/// running aggregate-sync accumulator - see core::chain::aggregate_validate
/// and total_registry_fees_burned for why both are needed.
fn extend_aggregate(kernels: &mut Vec<TxKernel>, registry_fees: &mut u64, block: &Block) {
    kernels.extend(block.body.kernels.iter().cloned());
    for op in &block.name_ops {
        kernels.extend(op.fee_payment.kernels.iter().cloned());
        *registry_fees += op.fee_payment.kernels.iter().map(|k| k.fee).sum::<u64>();
    }
    for op in &block.mint_ops {
        kernels.extend(op.fee_payment.kernels.iter().cloned());
        *registry_fees += op.fee_payment.kernels.iter().map(|k| k.fee).sum::<u64>();
    }
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

async fn handle_peer_connection(
    mut reader: PeerReader,
    write_half: Arc<TokioMutex<PeerWriter>>,
    peer_addr: String,
    pm: Arc<PeerManager>,
    mempool: Arc<Mutex<Mempool>>,
    chain: Arc<Mutex<ChainState>>,
    storage: Arc<Storage>,
    marketplace: Arc<MarketplaceState>,
    allowlist: Arc<AllowlistState>,
    router: Arc<DandelionRouter>,
    own_listen_addr: String,
) {
    // State for a sync that's fallen back to aggregate validation (see
    // core::chain::aggregate_validate) because a received block failed its
    // own per-block balance re-check - the peer horizon-pruned it (see
    // core::compaction). Persists across multiple BlocksBatch messages
    // since one sync typically spans more than SYNC_BATCH_SIZE blocks.
    let mut aggregate_mode = false;
    let mut aggregate_kernels: Vec<TxKernel> = Vec::new();
    let mut aggregate_registry_fees: u64 = 0;
    // Hash of the last block folded into the aggregate accumulator, so each
    // subsequent block's prev_hash can be checked against it directly -
    // aggregate-mode blocks don't advance the local chain tip, so
    // chain.last_block_hash can't be used for this once mode is entered.
    let mut aggregate_prev_hash: Option<[u8; 32]> = None;

    loop {
        match transport::read_message(&mut reader).await {
            Some(msg) => {
                    match msg {
                        P2pMessage::Handshake { listen_addr } => {
                            println!("P2P: Handshake received from {} (listening on {})", peer_addr, listen_addr);
                            pm.add_known_peer(listen_addr);
                            let (our_height, our_tip) = {
                                let cs = chain.lock().unwrap();
                                (cs.current_height, cs.last_block_hash)
                            };
                            let mut w = write_half.lock().await;
                            let _ = transport::write_message(&mut *w, &P2pMessage::ChainInfo { height: our_height, tip_hash: our_tip }).await;
                            let _ = transport::write_message(&mut *w, &P2pMessage::GetPeers).await;
                        }
                        P2pMessage::Ping => {
                            let mut w = write_half.lock().await;
                            let _ = transport::write_message(&mut *w, &P2pMessage::Pong).await;
                        }
                        P2pMessage::Pong => {
                            // Alive confirmation
                        }
                        P2pMessage::StemTx(tx) => {
                            println!("P2P: Received StemTx {:?} from {}", compute_tx_id(&tx), peer_addr);
                            dispatch_dandelion_tx(tx, false, &mempool, &pm, &router).await;
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
                                // Clear spent mempool transactions and stale name ops
                                {
                                    let mut mp = mempool.lock().unwrap();
                                    mp.clear_spent(&block.body);
                                    let registered_names: Vec<String> = block.name_ops.iter().map(|op| op.name.clone()).collect();
                                    let spent: Vec<Commitment> = block.name_ops.iter().flat_map(|op| op.fee_payment.inputs.iter().map(|i| i.commitment)).collect();
                                    mp.clear_stale_name_ops(&registered_names, &spent);
                                    let touched_names: Vec<String> = registered_names.iter().cloned()
                                        .chain(block.transfer_ops.iter().map(|op| op.name.clone()))
                                        .collect();
                                    mp.clear_stale_transfer_ops(&touched_names);
                                    let minted_assets: Vec<String> = block.mint_ops.iter().map(|op| op.asset_id.clone()).collect();
                                    let mint_spent: Vec<Commitment> = block.mint_ops.iter().flat_map(|op| op.fee_payment.inputs.iter().map(|i| i.commitment)).collect();
                                    mp.clear_stale_mint_ops(&minted_assets, &mint_spent);
                                    let touched_assets: Vec<String> = minted_assets.iter().cloned()
                                        .chain(block.transfer_asset_ops.iter().map(|op| op.asset_id.clone()))
                                        .collect();
                                    mp.clear_stale_transfer_asset_ops(&touched_assets);
                                    let launched_collections: Vec<String> = block.launch_collection_ops.iter().map(|op| op.collection_id.clone()).collect();
                                    mp.clear_stale_launch_collection_ops(&launched_collections);
                                    let registered_validators: Vec<Commitment> = block.validator_ops.iter().map(|op| op.commitment).collect();
                                    mp.clear_stale_validator_ops(&registered_validators);
                                    let asset_registry_snapshot = chain.lock().unwrap().asset_registry.clone();
                                    marketplace.clear_stale(&touched_assets, &asset_registry_snapshot);
                                }
                                // Propagate block
                                pm.broadcast(&P2pMessage::NewBlock(block)).await;
                            }
                        }
                        P2pMessage::NewValidatorOp(op) => {
                            let commitment = op.commitment;
                            let added = {
                                let mut mp = mempool.lock().unwrap();
                                mp.add_validator_op(op.clone())
                            };
                            if added {
                                println!("P2P: Queued stake registration for commitment {:?} from {}, propagating.", commitment, peer_addr);
                                pm.broadcast(&P2pMessage::NewValidatorOp(op)).await;
                            }
                        }
                        P2pMessage::NewNameOp(op) => {
                            let name = op.name.clone();
                            let added = {
                                let mut mp = mempool.lock().unwrap();
                                mp.add_name_op(op.clone())
                            };
                            if added {
                                println!("P2P: Queued name registration '{}' from {}, propagating.", name, peer_addr);
                                pm.broadcast(&P2pMessage::NewNameOp(op)).await;
                            }
                        }
                        P2pMessage::NewTransferOp(op) => {
                            let name = op.name.clone();
                            let added = {
                                let mut mp = mempool.lock().unwrap();
                                mp.add_transfer_op(op.clone())
                            };
                            if added {
                                println!("P2P: Queued name transfer '{}' from {}, propagating.", name, peer_addr);
                                pm.broadcast(&P2pMessage::NewTransferOp(op)).await;
                            }
                        }
                        P2pMessage::NewMintOp(op) => {
                            let asset_id = op.asset_id.clone();
                            let added = {
                                let mut mp = mempool.lock().unwrap();
                                mp.add_mint_op(op.clone())
                            };
                            if added {
                                println!("P2P: Queued asset mint '{}' from {}, propagating.", asset_id, peer_addr);
                                pm.broadcast(&P2pMessage::NewMintOp(op)).await;
                            }
                        }
                        P2pMessage::NewTransferAssetOp(op) => {
                            let asset_id = op.asset_id.clone();
                            let added = {
                                let mut mp = mempool.lock().unwrap();
                                mp.add_transfer_asset_op(op.clone())
                            };
                            if added {
                                println!("P2P: Queued asset transfer '{}' from {}, propagating.", asset_id, peer_addr);
                                pm.broadcast(&P2pMessage::NewTransferAssetOp(op)).await;
                            }
                        }
                        P2pMessage::NewListing(listing) => {
                            let asset_id = listing.asset_id.clone();
                            let valid = listing.validate_standalone().is_ok() && {
                                let c = chain.lock().unwrap();
                                listing.validate_against_registry(&c.asset_registry).is_ok()
                            };
                            if valid {
                                marketplace.add_or_replace(listing.clone());
                                println!("P2P: Queued marketplace listing '{}' from {}, propagating.", asset_id, peer_addr);
                                pm.broadcast(&P2pMessage::NewListing(listing)).await;
                            }
                        }
                        P2pMessage::CancelListing { asset_id, seller_pubkey, signature } => {
                            let msg = crate::core::marketplace::cancel_signing_message(&asset_id, &seller_pubkey);
                            if signature.verify(&msg, &seller_pubkey) && marketplace.cancel(&asset_id, &seller_pubkey) {
                                println!("P2P: Cancelled marketplace listing '{}' from {}, propagating.", asset_id, peer_addr);
                                pm.broadcast(&P2pMessage::CancelListing { asset_id, seller_pubkey, signature }).await;
                            }
                        }
                        P2pMessage::NewLaunchCollectionOp(op) => {
                            let collection_id = op.collection_id.clone();
                            let added = {
                                let mut mp = mempool.lock().unwrap();
                                mp.add_launch_collection_op(op.clone())
                            };
                            if added {
                                println!("P2P: Queued collection launch '{}' from {}, propagating.", collection_id, peer_addr);
                                pm.broadcast(&P2pMessage::NewLaunchCollectionOp(op)).await;
                            }
                        }
                        P2pMessage::NewAllowlist(entry) => {
                            let collection_id = entry.collection_id.clone();
                            let phase_index = entry.phase_index;
                            let valid = entry.validate_standalone().is_ok() && {
                                let c = chain.lock().unwrap();
                                entry.validate_against_registry(&c.collection_registry).is_ok()
                            };
                            if valid {
                                allowlist.publish(entry.clone());
                                println!("P2P: Queued allowlist publish for '{}' phase {} from {}, propagating.", collection_id, phase_index, peer_addr);
                                pm.broadcast(&P2pMessage::NewAllowlist(entry)).await;
                            }
                        }
                        P2pMessage::ChainInfo { height, tip_hash } => {
                            let our_height = { chain.lock().unwrap().current_height };
                            println!("P2P: Peer {} reports chain height {} (tip {:?}, ours: {})", peer_addr, height, tip_hash, our_height);
                            let mut w = write_half.lock().await;
                            if height > our_height {
                                let _ = transport::write_message(&mut *w, &P2pMessage::GetBlocks { from_height: our_height + 1 }).await;
                            }
                            // Already caught up on blocks - active_validators is
                            // derived entirely from block replay now (see
                            // core::chain::RegisterValidatorOp), so no separate
                            // validator-set sync round-trip is needed.
                        }
                        P2pMessage::GetBlocks { from_height } => {
                            // Always serve whatever we have - headers and kernels
                            // are never stripped by compact() (only specific
                            // inputs/outputs are), so there's no range this node
                            // can't hand over; the requester falls back to
                            // aggregate_validate for anything it can't re-check
                            // per-block (see the BlocksBatch handler below).
                            let (blocks, has_more) = {
                                let cs = chain.lock().unwrap();
                                cs.get_blocks_from(from_height, SYNC_BATCH_SIZE)
                            };
                            println!("P2P: Sending {} blocks (from height {}) to {}", blocks.len(), from_height, peer_addr);
                            let mut w = write_half.lock().await;
                            let _ = transport::write_message(&mut *w, &P2pMessage::BlocksBatch { blocks, has_more }).await;
                        }
                        P2pMessage::PrunedRange { earliest_full_height } => {
                            // No longer sent under normal compaction - see the
                            // GetBlocks handler above. Kept as a no-op arm in
                            // case a future archival-pruning peer ever sends it.
                            println!("P2P: {} reports pruned below height {} (unexpected under current compaction).", peer_addr, earliest_full_height);
                        }
                        P2pMessage::BlocksBatch { blocks, has_more } => {
                            println!("P2P: Received batch of {} blocks from {} (has_more: {}, aggregate_mode: {})", blocks.len(), peer_addr, has_more, aggregate_mode);
                            let batch_len = blocks.len();
                            let mut applied_count = 0;
                            for block in &blocks {
                                if aggregate_mode {
                                    // Already falling back - only structural
                                    // sanity is checked (a block that fails this
                                    // is a real reject, not a pruning artifact);
                                    // the value equation itself is deferred to
                                    // the final aggregate_validate call.
                                    let links = aggregate_prev_hash.map(|h| block.header.prev_hash == h).unwrap_or(false);
                                    if !links || block.header.chain_id != crate::core::genesis::CHAIN_ID {
                                        println!("P2P: Aggregate-sync from {} broke structural sanity at block #{}, aborting.", peer_addr, block.header.height);
                                        aggregate_mode = false;
                                        aggregate_kernels.clear();
                                        aggregate_registry_fees = 0;
                                        aggregate_prev_hash = None;
                                        break;
                                    }
                                    extend_aggregate(&mut aggregate_kernels, &mut aggregate_registry_fees, block);
                                    aggregate_prev_hash = Some(block.header.hash());
                                    applied_count += 1;
                                    continue;
                                }

                                let result = {
                                    let mut c = chain.lock().unwrap();
                                    c.apply_block(block)
                                };
                                if result.is_applied() {
                                    applied_count += 1;
                                    persist_apply_result(&storage, &result);
                                    let mut mp = mempool.lock().unwrap();
                                    mp.clear_spent(&block.body);
                                    let registered_names: Vec<String> = block.name_ops.iter().map(|op| op.name.clone()).collect();
                                    let spent: Vec<Commitment> = block.name_ops.iter().flat_map(|op| op.fee_payment.inputs.iter().map(|i| i.commitment)).collect();
                                    mp.clear_stale_name_ops(&registered_names, &spent);
                                    let touched_names: Vec<String> = registered_names.iter().cloned()
                                        .chain(block.transfer_ops.iter().map(|op| op.name.clone()))
                                        .collect();
                                    mp.clear_stale_transfer_ops(&touched_names);
                                    let minted_assets: Vec<String> = block.mint_ops.iter().map(|op| op.asset_id.clone()).collect();
                                    let mint_spent: Vec<Commitment> = block.mint_ops.iter().flat_map(|op| op.fee_payment.inputs.iter().map(|i| i.commitment)).collect();
                                    mp.clear_stale_mint_ops(&minted_assets, &mint_spent);
                                    let touched_assets: Vec<String> = minted_assets.iter().cloned()
                                        .chain(block.transfer_asset_ops.iter().map(|op| op.asset_id.clone()))
                                        .collect();
                                    mp.clear_stale_transfer_asset_ops(&touched_assets);
                                    let launched_collections: Vec<String> = block.launch_collection_ops.iter().map(|op| op.collection_id.clone()).collect();
                                    mp.clear_stale_launch_collection_ops(&launched_collections);
                                    let registered_validators: Vec<Commitment> = block.validator_ops.iter().map(|op| op.commitment).collect();
                                    mp.clear_stale_validator_ops(&registered_validators);
                                    let asset_registry_snapshot = chain.lock().unwrap().asset_registry.clone();
                                    marketplace.clear_stale(&touched_assets, &asset_registry_snapshot);
                                } else {
                                    // Full apply failed - check whether this looks
                                    // like a horizon-pruning artifact (the chain
                                    // otherwise still links up correctly) rather
                                    // than a genuinely broken/malicious block.
                                    let links_ok = { block.header.prev_hash == chain.lock().unwrap().last_block_hash };
                                    if links_ok && block.header.chain_id == crate::core::genesis::CHAIN_ID {
                                        println!("P2P: Block #{} failed per-block re-validation (likely horizon-pruned) - falling back to aggregate sync from {}.", block.header.height, peer_addr);
                                        aggregate_mode = true;
                                        aggregate_kernels = chain.lock().unwrap().kernels.clone();
                                        aggregate_registry_fees = crate::core::chain::total_registry_fees_burned(&chain.lock().unwrap().blocks);
                                        extend_aggregate(&mut aggregate_kernels, &mut aggregate_registry_fees, block);
                                        aggregate_prev_hash = Some(block.header.hash());
                                        applied_count += 1;
                                    } else {
                                        println!("P2P: Sync block #{} failed to apply, stopping sync from {}", block.header.height, peer_addr);
                                        break;
                                    }
                                }
                            }
                            println!("P2P: Synced {} / {} blocks from {}", applied_count, batch_len, peer_addr);

                            if has_more && applied_count == batch_len {
                                let next_from = if aggregate_mode {
                                    blocks.last().map(|b| b.header.height + 1).unwrap_or(1)
                                } else {
                                    chain.lock().unwrap().current_height + 1
                                };
                                let mut w = write_half.lock().await;
                                let _ = transport::write_message(&mut *w, &P2pMessage::GetBlocks { from_height: next_from }).await;
                            } else if applied_count == batch_len && aggregate_mode {
                                // Reached the peer's reported tip while still
                                // mid-fallback - the UTXO set for the pruned
                                // range can't be rebuilt incrementally (a
                                // partially-pruned block's remaining in/outputs
                                // no longer represent its true diff), so fetch
                                // it as a snapshot and finish with one aggregate
                                // check instead.
                                let mut w = write_half.lock().await;
                                let _ = transport::write_message(&mut *w, &P2pMessage::GetUtxoSnapshot).await;
                            }
                            // Block sync just finished - active_validators is
                            // derived entirely from block replay now, no separate
                            // sync round-trip needed (see the ChainInfo handler).
                        }
                        P2pMessage::GetUtxoSnapshot => {
                            let (utxos, height, tip_hash) = {
                                let cs = chain.lock().unwrap();
                                (cs.utxos.iter().cloned().collect(), cs.current_height, cs.last_block_hash)
                            };
                            let mut w = write_half.lock().await;
                            let _ = transport::write_message(&mut *w, &P2pMessage::UtxoSnapshot { utxos, height, tip_hash }).await;
                        }
                        P2pMessage::UtxoSnapshot { utxos, height, tip_hash } => {
                            if !aggregate_mode {
                                println!("P2P: Received unexpected UtxoSnapshot from {} (not aggregate-syncing), ignoring.", peer_addr);
                            } else {
                                let utxo_set: HashSet<Commitment> = utxos.into_iter().collect();
                                let valid = crate::core::chain::aggregate_validate(&utxo_set, &aggregate_kernels, height, aggregate_registry_fees);
                                if valid {
                                    println!("P2P: Aggregate validation passed for {} (height {}) - adopting its UTXO snapshot.", peer_addr, height);
                                    let mut c = chain.lock().unwrap();
                                    c.utxos = utxo_set;
                                    c.current_height = height;
                                    c.last_block_hash = tip_hash;
                                } else {
                                    println!("P2P: Aggregate validation FAILED for {} (height {}) - rejecting its sync data entirely.", peer_addr, height);
                                }
                                aggregate_mode = false;
                                aggregate_kernels.clear();
                                aggregate_registry_fees = 0;
                                aggregate_prev_hash = None;
                            }
                        }
                        P2pMessage::GetPeers => {
                            let peers_list = pm.known_peers_snapshot();
                            let mut w = write_half.lock().await;
                            let _ = transport::write_message(&mut *w, &P2pMessage::PeersList(peers_list)).await;
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
                                let mkt2 = Arc::clone(&marketplace);
                                let al2 = Arc::clone(&allowlist);
                                let r2 = Arc::clone(&router);
                                let own_listen_addr2 = own_listen_addr.clone();
                                tokio::spawn(async move {
                                    connect_to_peer(candidate, pm2, mp2, c2, st2, mkt2, al2, r2, own_listen_addr2).await;
                                });
                            }
                        }
                    }
            }
            None => break,
        }
    }
    println!("P2P: Connection with {} closed.", peer_addr);
    pm.remove_peer(&peer_addr);
}
