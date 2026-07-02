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
    GetPeers,
    PeersList(Vec<String>),
    /// Requests the peer's current active validator set - sent once block
    /// sync completes, since active_validators isn't part of block history
    /// (it's only ever mutated live via RegisterValidator) and a node that
    /// joins/reconnects after a registration was broadcast would otherwise
    /// never learn about it.
    GetValidators,
    ValidatorsList(Vec<crate::core::chain::Validator>),
}
