//! `cargo-fuzz` needs a nightly toolchain + libFuzzer/clang, which is
//! fragile on Windows/MSVC and wasn't set up in this repo. This is a
//! pragmatic stand-in that runs on stable Rust via plain `cargo test`: it
//! throws random and bit-mutated byte strings at `P2pMessage`
//! deserialization - the actual entry point for raw, untrusted bytes off
//! the wire (see p2p::transport) - and asserts the process never panics.
//! It's not coverage-guided like a real fuzzer, so it won't find everything
//! a proper corpus-driven fuzz campaign would, but it's a meaningful floor:
//! a bad deserialize must return Err, never crash the node.

use haze_p2p::message::P2pMessage;
use rand::Rng;

const ITERATIONS: usize = 20_000;

#[test]
fn random_bytes_never_panic_the_deserializer() {
    let mut rng = rand::thread_rng();
    for _ in 0..ITERATIONS {
        let len = rng.gen_range(0..512);
        let bytes: Vec<u8> = (0..len).map(|_| rng.r#gen::<u8>()).collect();
        // Only the "doesn't panic" property is under test - Ok or Err are
        // both fine outcomes for arbitrary garbage.
        let _ = bincode::deserialize::<P2pMessage>(&bytes);
    }
}

#[test]
fn bit_flipped_valid_messages_never_panic_the_deserializer() {
    let mut rng = rand::thread_rng();

    let seeds: Vec<P2pMessage> = vec![
        P2pMessage::Ping,
        P2pMessage::Handshake { listen_addr: "127.0.0.1:9000".to_string() },
        P2pMessage::ChainInfo { height: 12345, tip_hash: [7u8; 32] },
        P2pMessage::GetBlocks { from_height: 999 },
        P2pMessage::BlocksBatch { blocks: vec![], has_more: false },
        P2pMessage::GetUtxoSnapshot,
        P2pMessage::PeersList(vec!["1.2.3.4:9000".to_string(), "5.6.7.8:9000".to_string()]),
    ];

    for seed in &seeds {
        let good_bytes = bincode::serialize(seed).expect("seed message must serialize");
        for _ in 0..(ITERATIONS / seeds.len().max(1)) {
            let mut mutated = good_bytes.clone();
            if mutated.is_empty() { continue; }
            // Flip a handful of random bits - enough to corrupt length
            // prefixes, enum discriminants, and payload bytes across runs.
            let flips = rng.gen_range(1..=4);
            for _ in 0..flips {
                let idx = rng.gen_range(0..mutated.len());
                let bit = rng.gen_range(0..8);
                mutated[idx] ^= 1 << bit;
            }
            let _ = bincode::deserialize::<P2pMessage>(&mutated);
        }
    }
}

#[test]
fn truncated_valid_messages_never_panic_the_deserializer() {
    let seed = P2pMessage::BlocksBatch { blocks: vec![], has_more: true };
    let good_bytes = bincode::serialize(&seed).expect("seed message must serialize");
    for cut in 0..=good_bytes.len() {
        let _ = bincode::deserialize::<P2pMessage>(&good_bytes[..cut]);
    }
}
