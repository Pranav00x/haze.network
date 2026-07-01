use serde::{Serialize, Deserialize};
use crate::core::transaction::Transaction;
use crate::core::block::Block;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum P2pMessage {
    Handshake { listen_addr: String },
    Ping,
    Pong,
    StemTx(Transaction),
    FluffTx(Transaction),
    NewBlock(Block),
    RegisterValidator {
        commitment: crate::crypto::pedersen::Commitment,
        value: u64,
        blinding: curve25519_dalek_ng::scalar::Scalar,
    },
    ChainInfo { height: u64, tip_hash: [u8; 32] },
    GetBlocks { from_height: u64 },
    BlocksBatch { blocks: Vec<Block>, has_more: bool },
}
