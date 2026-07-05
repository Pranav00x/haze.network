use serde::{Serialize, Deserialize};
use crate::core::transaction::Transaction;
use crate::core::block::Block;
use crate::core::registry::{RegisterNameOp, TransferNameOp};
use crate::core::assets::{MintAssetOp, TransferAssetOp};

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
    /// No longer sent under normal compaction - headers and kernels are
    /// never stripped by compact() (only specific inputs/outputs are), so a
    /// peer can always serve BlocksBatch for any range it has at all; the
    /// requester falls back to aggregate_validate (see core::chain) for any
    /// block whose own balance equation fails to re-check due to pruning.
    /// Kept defined, unused, in case a future archival-pruning mode ever
    /// discards headers too.
    PrunedRange { earliest_full_height: u64 },
    /// Requests the peer's current live UTXO set (plus the height/tip hash
    /// it corresponds to) - needed to complete a sync that fell back to
    /// aggregate_validate for part of its history, since a partially-pruned
    /// block's remaining inputs/outputs no longer represent that specific
    /// block's true diff, so the UTXO set for that range can't be rebuilt
    /// incrementally the way kernels can (kernels are never stripped).
    GetUtxoSnapshot,
    UtxoSnapshot { utxos: Vec<crate::crypto::pedersen::Commitment>, height: u64, tip_hash: [u8; 32] },
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
    /// A pending asset mint (see core::assets) - gossiped the same way as
    /// NewNameOp, same reasoning (asset ownership is intentionally public).
    NewMintOp(MintAssetOp),
    /// A pending asset transfer, gossiped the same way as NewTransferOp.
    NewTransferAssetOp(TransferAssetOp),
}
