use serde::{Serialize, Deserialize};
use haze_chain::transaction::Transaction;
use haze_chain::block::Block;
use haze_chain::registry::{RegisterNameOp, TransferNameOp};
use haze_chain::assets::{MintAssetOp, TransferAssetOp};
use haze_chain::marketplace::Listing;
use haze_chain::collections::LaunchCollectionOp;
use haze_chain::allowlist::AllowlistEntry;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum P2pMessage {
    Handshake { listen_addr: String },
    Ping,
    Pong,
    StemTx(Transaction),
    FluffTx(Transaction),
    NewBlock(Block),
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
    UtxoSnapshot { utxos: Vec<haze_crypto::pedersen::Commitment>, height: u64, tip_hash: [u8; 32] },
    GetPeers,
    PeersList(Vec<String>),
    /// A pending stake registration (see core::chain::RegisterValidatorOp) -
    /// gossiped the same way as NewMintOp/NewLaunchCollectionOp. Only takes
    /// effect once included in a block (see Block::validator_ops) - unlike
    /// the old RegisterValidator message this replaced, receiving this does
    /// NOT mutate active_validators directly, so every node derives the
    /// same validator set purely from block content/order, not from
    /// whatever order registrations happened to arrive over the network.
    NewValidatorOp(haze_chain::chain::RegisterValidatorOp),
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
    /// A marketplace listing (see core::marketplace) - gossiped the same
    /// way, no Dandelion (a listing is meant to be publicly discoverable).
    NewListing(Listing),
    /// Cancels a previously-gossiped listing. Carries its own signature
    /// (over asset_id + seller_pubkey) rather than reusing Listing's,
    /// since cancellation is a distinct signed statement ("I withdraw
    /// this") from the listing itself.
    CancelListing {
        asset_id: String,
        seller_pubkey: haze_crypto::pedersen::Commitment,
        signature: haze_crypto::schnorr::Signature,
    },
    /// A pending collection launch (see core::collections) - gossiped the
    /// same way as NewMintOp, same reasoning (a drop's schedule is
    /// intentionally public).
    NewLaunchCollectionOp(LaunchCollectionOp),
    /// An off-chain allowlist publish for one collection phase (see
    /// core::allowlist) - gossiped the same way as NewListing, no Dandelion
    /// (an allowlist is meant to be publicly fetchable so any client can
    /// compute its own Merkle proof).
    NewAllowlist(AllowlistEntry),
}
