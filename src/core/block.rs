use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;
use super::transaction::Transaction;
use super::registry::{RegisterNameOp, TransferNameOp};
use super::assets::{MintAssetOp, TransferAssetOp};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub prev_hash: [u8; 32],
    // Mimblewimble offset for the block
    pub total_kernel_offset: curve25519_dalek_ng::scalar::Scalar,
    pub nonce: u64,
    pub timestamp: u64,
    pub validator_commitment: Commitment,
    pub validator_signature: Signature,
    /// Commitment to the full naming-registry state after this block is
    /// applied (see core::registry::compute_registry_root). Separate from
    /// the UTXO set on purpose - name ownership is intentionally public.
    pub name_registry_root: [u8; 32],
    /// Identifies which network this block belongs to (see
    /// core::genesis::CHAIN_ID/NETWORK_NAME) - part of the hash so nodes on
    /// different networks can never accidentally interoperate, even if they
    /// somehow connected over P2P (a mismatched genesis block hash alone
    /// already prevents this in practice, but this makes the intent
    /// explicit and checkable on every block, not just genesis).
    pub chain_id: u64,
    /// Commitment to the full asset-registry state after this block is
    /// applied (see core::assets::compute_asset_registry_root) - same
    /// pattern and same reasoning as name_registry_root, kept as a separate
    /// field/root since assets and names are unrelated namespaces.
    pub asset_registry_root: [u8; 32],
}

impl BlockHeader {
    pub fn hash(&self) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.prev_hash);
        hasher.update(self.total_kernel_offset.as_bytes());
        hasher.update(&self.nonce.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(self.validator_commitment.as_point().compress().as_bytes());
        hasher.update(self.validator_signature.s.as_bytes());
        hasher.update(self.validator_signature.e.as_bytes());
        hasher.update(&self.name_registry_root);
        hasher.update(&self.chain_id.to_le_bytes());
        hasher.update(&self.asset_registry_root);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}

/// Bitcoin-style integer halving schedule (see core::genesis's tokenomics
/// lock for the full reasoning): reward is cut in half every
/// HALVING_INTERVAL_BLOCKS, eventually reaching (and permanently staying at)
/// zero. Height 0 is genesis, minted separately via
/// core::genesis::GENESIS_TOTAL_MINTED - this function is never consulted
/// for it.
pub fn block_reward_at(height: u64) -> u64 {
    let halvings = height / super::genesis::HALVING_INTERVAL_BLOCKS;
    if halvings >= 64 {
        return 0; // shift amount would overflow u64 - schedule has long since reached 0 anyway
    }
    super::genesis::INITIAL_BLOCK_REWARD >> halvings
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    // The entire block is just one giant cut-through transaction
    pub body: Transaction,
    /// Name registrations included in this block - separate from `body`
    /// since they're not a value-transfer operation, just carry their own
    /// small fee-paying transaction each (see RegisterNameOp).
    #[serde(default)]
    pub name_ops: Vec<RegisterNameOp>,
    /// Name ownership transfers included in this block (see TransferNameOp) -
    /// kept separate from name_ops since they have no fee-payment sub-transaction.
    #[serde(default)]
    pub transfer_ops: Vec<TransferNameOp>,
    /// Asset mints included in this block (see core::assets::MintAssetOp) -
    /// same shape as name_ops, a separate namespace from names.
    #[serde(default)]
    pub mint_ops: Vec<MintAssetOp>,
    /// Asset ownership transfers included in this block (see
    /// core::assets::TransferAssetOp) - same shape as transfer_ops.
    #[serde(default)]
    pub transfer_asset_ops: Vec<TransferAssetOp>,
}

impl Block {
    /// Validates the block by checking its internal transaction balances and signatures.
    /// Name ops are validated separately (ChainState::apply_linear_block) since checking
    /// them fully requires chain state (name uniqueness, real UTXOs) this method doesn't have.
    pub fn validate(&self) -> bool {
        let reward = if self.header.height == 0 {
            super::genesis::GENESIS_TOTAL_MINTED
        } else {
            block_reward_at(self.header.height)
        };
        self.body.validate_with_reward(reward)
    }
}
