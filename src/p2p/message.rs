use serde::{Serialize, Deserialize};
use crate::core::transaction::Transaction;
use crate::core::block::Block;
use crate::core::registry::{RegisterNameOp, TransferNameOp};

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
    /// Sent instead of BlocksBatch when `from_height` reaches into territory
    /// this node has already horizon-compacted (see core::compaction) -
    /// those blocks are missing some inputs/outputs, so a peer trying to
    /// fully re-validate chain state from scratch through them would fail
    /// their per-block balance check. `earliest_full_height` is the oldest
    /// height this node CAN still serve completely; the requester needs a
    /// different (less-compacted or archival) peer for anything older.
    PrunedRange { earliest_full_height: u64 },
    GetPeers,
    PeersList(Vec<String>),
    /// Requests the peer's current active validator set - sent once block
    /// sync completes, since active_validators isn't part of block history
    /// (it's only ever mutated live via RegisterValidator) and a node that
    /// joins/reconnects after a registration was broadcast would otherwise
    /// never learn about it.
    GetValidators,
    ValidatorsList(Vec<crate::core::chain::Validator>),
    /// A pending name registration, gossiped directly (not via Dandelion
    /// stem/fluff - unlike payment transactions, name ownership is
    /// intentionally public, so there's no privacy benefit to routing it
    /// through the stem phase).
    NewNameOp(RegisterNameOp),
    /// A pending name transfer, gossiped the same way (no Dandelion).
    NewTransferOp(TransferNameOp),
}
