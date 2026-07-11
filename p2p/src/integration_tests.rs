use haze_chain::chain::ChainState;
use haze_crypto::pedersen::Commitment;
use haze_crypto::schnorr::Signature;

use curve25519_dalek_ng::scalar::Scalar;
use rand::rngs::OsRng;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::sync::Arc;

/// Builds the ownership proof register_validator requires in place of a raw
/// blinding factor - see haze_chain::chain::stake_registration_message.
fn stake_proof(commitment: &Commitment, value: u64, blinding: &Scalar) -> Signature {
    let msg = haze_chain::chain::stake_registration_message(commitment, value);
    Signature::sign(&msg, blinding)
}

#[tokio::test]
async fn test_dandelion_fluff_timeout() {
    use crate::dandelion::DandelionRouter;

    let router = DandelionRouter::new(0.0); // 0% fluff probability
    let tx_id = [7u8; 32];
    let fluffed = Arc::new(AtomicBool::new(false));

    let fluffed_clone = Arc::clone(&fluffed);
    router.register_stem_tx(tx_id, 1, move || {
        fluffed_clone.store(true, Ordering::SeqCst);
    });

    // Initially should not be fluffed
    assert!(!fluffed.load(Ordering::SeqCst));

    // Sleep to let timer expire (1s timeout)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Should now be fluffed via fallback trigger
    assert!(fluffed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_p2p_validator_propagation() {
    use tokio::io::AsyncWriteExt;

    let mut rng = OsRng;
    let mempool = Arc::new(std::sync::Mutex::new(haze_chain::mempool::Mempool::new()));
    let chain_state = Arc::new(std::sync::Mutex::new(ChainState::new()));

    // 1. Create a validator commitment and add it to the UTXO set
    let r = Scalar::random(&mut rng);
    let value = 2500u64;
    let commitment = Commitment::new(value, r);
    {
        let mut c = chain_state.lock().unwrap();
        c.utxos.insert(commitment);
    }

    // 2. Start the P2pServer
    let test_db_path = format!("{}/haze_test_db_{}", std::env::temp_dir().display(), std::process::id());
    let storage = Arc::new(haze_chain::storage::Storage::open_at(&test_db_path));
    let marketplace_state = Arc::new(haze_chain::marketplace::MarketplaceState::new());
    let allowlist_state = Arc::new(haze_chain::allowlist::AllowlistState::new());
    let p2p_server = Arc::new(crate::server::P2pServer::new(Arc::clone(&mempool), Arc::clone(&chain_state), storage, marketplace_state, allowlist_state));
    let server_clone = Arc::clone(&p2p_server);

    // Find a random free port and bind
    let bind_addr = "127.0.0.1:28333";
    tokio::spawn(async move {
        let _ = server_clone.start(bind_addr, vec![]).await;
    });

    // Sleep to let P2P server start listening
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 3. Connect as a client TcpStream
    let mut client_stream = tokio::net::TcpStream::connect(bind_addr).await.unwrap();

    // 4. Send Handshake
    let handshake = crate::message::P2pMessage::Handshake {
        listen_addr: "127.0.0.1:28334".to_string(),
    };
    let bytes = bincode::serialize(&handshake).unwrap();
    let len = bytes.len() as u32;
    client_stream.write_all(&len.to_le_bytes()).await.unwrap();
    client_stream.write_all(&bytes).await.unwrap();
    client_stream.flush().await.unwrap();

    // 5. Send NewValidatorOp - stake registration is now queued into the
    // mempool (like every other op) rather than mutating chain state
    // directly, so it only becomes an active validator once mined into
    // a block; this test verifies the P2P propagation/queueing step.
    let op = haze_chain::chain::RegisterValidatorOp {
        commitment,
        value,
        proof: stake_proof(&commitment, value, &r),
    };
    let reg_msg = crate::message::P2pMessage::NewValidatorOp(op);
    let bytes = bincode::serialize(&reg_msg).unwrap();
    let len = bytes.len() as u32;
    client_stream.write_all(&len.to_le_bytes()).await.unwrap();
    client_stream.write_all(&bytes).await.unwrap();
    client_stream.flush().await.unwrap();

    // Sleep to let server handle message
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 6. Verify that the stake registration was queued into the mempool!
    let mut mp = mempool.lock().unwrap();
    let queued = mp.take_validator_ops();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].commitment, commitment);
    assert_eq!(queued[0].value, value);
}
