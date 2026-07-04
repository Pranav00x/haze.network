use super::transaction::Transaction;
use super::cut_through::aggregate_and_cut_through;
use super::registry::{RegisterNameOp, TransferNameOp, NAME_REGISTRATION_FEE};
use super::assets::{MintAssetOp, TransferAssetOp, ASSET_MINT_FEE};

/// Caps how many name registrations/transfers can land in a single block,
/// bounding block size the same way SYNC_BATCH_SIZE bounds a sync batch
/// elsewhere.
pub const MAX_NAME_OPS_PER_BLOCK: usize = 10;
pub const MAX_TRANSFER_OPS_PER_BLOCK: usize = 10;
/// Same reasoning as MAX_NAME_OPS_PER_BLOCK, separate namespace.
pub const MAX_MINT_OPS_PER_BLOCK: usize = 10;
pub const MAX_TRANSFER_ASSET_OPS_PER_BLOCK: usize = 10;

/// Caps how many ordinary payment transactions land in a single block. Before
/// this existed, aggregate() pulled in the ENTIRE mempool unconditionally -
/// with no cap, block space was never actually scarce, so there was nothing
/// for a fee to buy: every transaction got in on the very next block
/// regardless of what it paid. This cap is what makes fee-based
/// prioritization (see aggregate()) and congestion pricing (see
/// suggested_fee()) mean anything at all.
pub const MAX_TXS_PER_BLOCK: usize = 50;

/// Policy-level floor enforced at mempool acceptance (add_transaction) - NOT
/// a hard consensus rule checked at block-apply time, same tier as Bitcoin's
/// own minimum relay fee. A malicious proposer assembling their own block
/// could still include a below-floor transaction; this only stops one from
/// entering an honest node's mempool (and thus its own future blocks).
pub const MIN_FEE: u64 = 5;

fn tx_fee(tx: &Transaction) -> u64 {
    tx.kernels.iter().map(|k| k.fee).sum()
}

fn name_op_fee(op: &RegisterNameOp) -> u64 {
    tx_fee(&op.fee_payment)
}

fn mint_op_fee(op: &MintAssetOp) -> u64 {
    tx_fee(&op.fee_payment)
}

pub struct Mempool {
    pending_txs: Vec<Transaction>,
    pending_name_ops: Vec<RegisterNameOp>,
    pending_transfer_ops: Vec<TransferNameOp>,
    pending_mint_ops: Vec<MintAssetOp>,
    pending_transfer_asset_ops: Vec<TransferAssetOp>,
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            pending_txs: Vec::new(),
            pending_name_ops: Vec::new(),
            pending_transfer_ops: Vec::new(),
            pending_mint_ops: Vec::new(),
            pending_transfer_asset_ops: Vec::new(),
        }
    }

    /// Validates and adds a transaction to the mempool
    pub fn add_transaction(&mut self, tx: Transaction) -> bool {
        if !tx.validate() {
            return false;
        }
        if tx_fee(&tx) < MIN_FEE {
            return false;
        }
        self.pending_txs.push(tx);
        true
    }

    /// Selects up to MAX_TXS_PER_BLOCK pending transactions, highest total
    /// fee first, and cuts-through the selection. Anything left over stays
    /// in the mempool for the next block - unlike before, this is no longer
    /// "take everything," so a higher fee now genuinely buys earlier
    /// inclusion whenever the mempool has a real backlog.
    pub fn aggregate(&mut self) -> Option<Transaction> {
        if self.pending_txs.is_empty() {
            return None;
        }

        self.pending_txs.sort_by_key(|tx| std::cmp::Reverse(tx_fee(tx)));
        let take_n = self.pending_txs.len().min(MAX_TXS_PER_BLOCK);
        let selected: Vec<Transaction> = self.pending_txs.drain(0..take_n).collect();
        Some(aggregate_and_cut_through(selected))
    }

    /// A simple congestion-priced fee suggestion for wallets: MIN_FEE while
    /// the mempool comfortably fits in the next block, rising by one more
    /// MIN_FEE step for every additional full block's worth of backlog
    /// beyond that. Self-correcting - once the backlog drains (blocks keep
    /// coming and clearing it), the suggestion drops back down on its own.
    pub fn suggested_fee(&self) -> u64 {
        let backlog = self.pending_txs.len();
        let steps = backlog.div_ceil(MAX_TXS_PER_BLOCK).max(1);
        MIN_FEE * steps as u64
    }

    /// Number of transactions currently pending in the mempool.
    pub fn len(&self) -> usize {
        self.pending_txs.len()
    }

    /// Removes pending transactions that spend any outputs spent in the given block transaction.
    pub fn clear_spent(&mut self, block_tx: &Transaction) {
        use std::collections::HashSet;
        let spent: HashSet<_> = block_tx.inputs.iter().map(|i| i.commitment).collect();
        self.pending_txs.retain(|tx| {
            tx.inputs.iter().all(|i| !spent.contains(&i.commitment))
        });
    }

    /// Validates a name registration standalone (chain-state checks - name
    /// uniqueness, real UTXOs - happen again at block-apply time, same as
    /// how add_transaction only checks a Transaction's own internal balance).
    pub fn add_name_op(&mut self, op: RegisterNameOp) -> bool {
        if op.validate_standalone().is_err() {
            return false;
        }
        if self.pending_name_ops.iter().any(|o| o.name == op.name) {
            return false;
        }
        self.pending_name_ops.push(op);
        true
    }

    /// Drains up to MAX_NAME_OPS_PER_BLOCK pending name ops for inclusion in
    /// the next block, highest fee_payment fee first - same priority-by-fee
    /// idea as aggregate(), so paying above the NAME_REGISTRATION_FEE floor
    /// (see suggested_name_fee) actually buys earlier inclusion once the
    /// name-op backlog exceeds one block's worth.
    pub fn take_name_ops(&mut self) -> Vec<RegisterNameOp> {
        self.pending_name_ops.sort_by_key(|op| std::cmp::Reverse(name_op_fee(op)));
        let n = self.pending_name_ops.len().min(MAX_NAME_OPS_PER_BLOCK);
        self.pending_name_ops.drain(0..n).collect()
    }

    pub fn name_ops_len(&self) -> usize {
        self.pending_name_ops.len()
    }

    /// Congestion-priced suggestion for name registrations - same model as
    /// suggested_fee(), just against the name-op backlog vs
    /// MAX_NAME_OPS_PER_BLOCK instead of the payment backlog. The
    /// NAME_REGISTRATION_FEE floor itself is a real consensus rule (see its
    /// doc comment), but nothing stops a registration from voluntarily paying
    /// more to jump the queue when it's busy.
    pub fn suggested_name_fee(&self) -> u64 {
        let backlog = self.pending_name_ops.len();
        let steps = backlog.div_ceil(MAX_NAME_OPS_PER_BLOCK).max(1);
        NAME_REGISTRATION_FEE * steps as u64
    }

    /// Drops any still-pending name ops that a just-applied block has made
    /// stale: either the name got taken (by this op or a race), or its
    /// fee-payment input got spent elsewhere.
    pub fn clear_stale_name_ops(&mut self, registered_names: &[String], spent_commitments: &[crate::crypto::pedersen::Commitment]) {
        use std::collections::HashSet;
        let names: HashSet<&String> = registered_names.iter().collect();
        let spent: HashSet<_> = spent_commitments.iter().collect();
        self.pending_name_ops.retain(|op| {
            !names.contains(&op.name)
                && op.fee_payment.inputs.iter().all(|i| !spent.contains(&i.commitment))
        });
    }

    /// Queues a name transfer. Unlike add_name_op there's no useful standalone
    /// check (the signature can only be verified against the name's current
    /// owner, which requires chain state) - that happens again at block-
    /// assembly/apply time, same pattern as everything else here.
    pub fn add_transfer_op(&mut self, op: TransferNameOp) -> bool {
        if self.pending_transfer_ops.iter().any(|o| o.name == op.name) {
            return false;
        }
        self.pending_transfer_ops.push(op);
        true
    }

    /// Drains up to MAX_TRANSFER_OPS_PER_BLOCK pending transfer ops for
    /// inclusion in the next block.
    pub fn take_transfer_ops(&mut self) -> Vec<TransferNameOp> {
        let n = self.pending_transfer_ops.len().min(MAX_TRANSFER_OPS_PER_BLOCK);
        self.pending_transfer_ops.drain(0..n).collect()
    }

    /// Drops any still-pending transfer ops targeting a name that a
    /// just-applied block already touched (registered, transferred, or
    /// re-transferred) - it's stale either way, since the current owner
    /// (and thus valid signer) has changed or the target no longer exists
    /// the way this op assumed.
    pub fn clear_stale_transfer_ops(&mut self, touched_names: &[String]) {
        use std::collections::HashSet;
        let touched: HashSet<&String> = touched_names.iter().collect();
        self.pending_transfer_ops.retain(|op| !touched.contains(&op.name));
    }

    /// Validates an asset mint standalone - same pattern as add_name_op.
    pub fn add_mint_op(&mut self, op: MintAssetOp) -> bool {
        if op.validate_standalone().is_err() {
            return false;
        }
        if self.pending_mint_ops.iter().any(|o| o.asset_id == op.asset_id) {
            return false;
        }
        self.pending_mint_ops.push(op);
        true
    }

    /// Drains up to MAX_MINT_OPS_PER_BLOCK pending mint ops, highest
    /// fee_payment fee first - same priority-by-fee idea as take_name_ops.
    pub fn take_mint_ops(&mut self) -> Vec<MintAssetOp> {
        self.pending_mint_ops.sort_by_key(|op| std::cmp::Reverse(mint_op_fee(op)));
        let n = self.pending_mint_ops.len().min(MAX_MINT_OPS_PER_BLOCK);
        self.pending_mint_ops.drain(0..n).collect()
    }

    pub fn mint_ops_len(&self) -> usize {
        self.pending_mint_ops.len()
    }

    /// Congestion-priced suggestion for asset mints - same model as
    /// suggested_name_fee.
    pub fn suggested_asset_fee(&self) -> u64 {
        let backlog = self.pending_mint_ops.len();
        let steps = backlog.div_ceil(MAX_MINT_OPS_PER_BLOCK).max(1);
        ASSET_MINT_FEE * steps as u64
    }

    /// Drops any still-pending mint ops that a just-applied block has made
    /// stale - same pattern as clear_stale_name_ops.
    pub fn clear_stale_mint_ops(&mut self, minted_assets: &[String], spent_commitments: &[crate::crypto::pedersen::Commitment]) {
        use std::collections::HashSet;
        let minted: HashSet<&String> = minted_assets.iter().collect();
        let spent: HashSet<_> = spent_commitments.iter().collect();
        self.pending_mint_ops.retain(|op| {
            !minted.contains(&op.asset_id)
                && op.fee_payment.inputs.iter().all(|i| !spent.contains(&i.commitment))
        });
    }

    /// Queues an asset transfer - same pattern as add_transfer_op.
    pub fn add_transfer_asset_op(&mut self, op: TransferAssetOp) -> bool {
        if self.pending_transfer_asset_ops.iter().any(|o| o.asset_id == op.asset_id) {
            return false;
        }
        self.pending_transfer_asset_ops.push(op);
        true
    }

    /// Drains up to MAX_TRANSFER_ASSET_OPS_PER_BLOCK pending asset transfers.
    pub fn take_transfer_asset_ops(&mut self) -> Vec<TransferAssetOp> {
        let n = self.pending_transfer_asset_ops.len().min(MAX_TRANSFER_ASSET_OPS_PER_BLOCK);
        self.pending_transfer_asset_ops.drain(0..n).collect()
    }

    /// Drops any still-pending asset transfers targeting an asset a
    /// just-applied block already touched - same pattern as
    /// clear_stale_transfer_ops.
    pub fn clear_stale_transfer_asset_ops(&mut self, touched_assets: &[String]) {
        use std::collections::HashSet;
        let touched: HashSet<&String> = touched_assets.iter().collect();
        self.pending_transfer_asset_ops.retain(|op| !touched.contains(&op.asset_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::transaction::{Input, Output, TxKernel};
    use crate::crypto::pedersen::Commitment;
    use crate::crypto::range_proof::RangeProof;
    use crate::crypto::schnorr::Signature;
    use curve25519_dalek_ng::scalar::Scalar;
    use rand::rngs::OsRng;

    /// A minimal, cryptographically valid, self-contained transaction paying
    /// exactly `fee` - independent random blindings each call, so distinct
    /// instances never accidentally cut-through against each other.
    fn make_valid_tx_with_fee(fee: u64) -> Transaction {
        let mut rng = OsRng;
        let r_in = Scalar::random(&mut rng);
        let r_out = Scalar::random(&mut rng);
        let input_value = 1000 + fee;
        let output_value = 1000;

        let input = Input { commitment: Commitment::new(input_value, r_in) };
        let output_commitment = Commitment::new(output_value, r_out);
        let output_proof = RangeProof::prove(output_value, &r_out);
        let output = Output { commitment: output_commitment, proof: output_proof, note: vec![] };

        let excess_blinding = r_in - r_out;
        let excess = Commitment::new(0, excess_blinding);
        let signature = Signature::sign(&fee.to_le_bytes(), &excess_blinding);
        let kernel = TxKernel { excess, fee, signature };

        Transaction { inputs: vec![input], outputs: vec![output], kernels: vec![kernel] }
    }

    #[test]
    fn add_transaction_rejects_below_min_fee() {
        let mut mempool = Mempool::new();
        assert!(!mempool.add_transaction(make_valid_tx_with_fee(MIN_FEE - 1)));
        assert_eq!(mempool.len(), 0);
    }

    #[test]
    fn add_transaction_accepts_min_fee_and_above() {
        let mut mempool = Mempool::new();
        assert!(mempool.add_transaction(make_valid_tx_with_fee(MIN_FEE)));
        assert!(mempool.add_transaction(make_valid_tx_with_fee(MIN_FEE * 3)));
        assert_eq!(mempool.len(), 2);
    }

    #[test]
    fn aggregate_takes_highest_fee_first_and_leaves_the_rest() {
        let mut mempool = Mempool::new();
        let total = MAX_TXS_PER_BLOCK + 5;
        for i in 0..total {
            // Distinct, ascending fees - the top MAX_TXS_PER_BLOCK by fee are
            // exactly the last MAX_TXS_PER_BLOCK added here.
            mempool.add_transaction(make_valid_tx_with_fee(MIN_FEE + i as u64));
        }
        assert_eq!(mempool.len(), total);

        let aggregated = mempool.aggregate().expect("non-empty mempool must aggregate");
        assert_eq!(aggregated.kernels.len(), MAX_TXS_PER_BLOCK);
        assert_eq!(mempool.len(), 5, "the 5 lowest-fee transactions should remain queued");

        let min_fee_included: u64 = aggregated.kernels.iter().map(|k| k.fee).min().unwrap();
        // The lowest fee that made it into this block must still be higher
        // than every fee left behind in the mempool.
        assert!(min_fee_included > MIN_FEE + 4);
    }

    #[test]
    fn suggested_fee_rises_with_backlog_and_floors_at_min_fee() {
        let mut mempool = Mempool::new();
        assert_eq!(mempool.suggested_fee(), MIN_FEE, "empty mempool suggests the floor");

        for _ in 0..MAX_TXS_PER_BLOCK {
            mempool.add_transaction(make_valid_tx_with_fee(MIN_FEE));
        }
        assert_eq!(mempool.suggested_fee(), MIN_FEE, "exactly one block's worth is still the floor");

        mempool.add_transaction(make_valid_tx_with_fee(MIN_FEE));
        assert_eq!(mempool.suggested_fee(), MIN_FEE * 2, "one more than a full block bumps the suggestion");
    }

    /// Mirrors make_valid_tx_with_fee, but wrapped as a RegisterNameOp -
    /// independent random owner key + name each call so distinct instances
    /// never collide on add_name_op's per-name uniqueness check.
    fn make_valid_name_op_with_fee(name: &str, fee: u64) -> RegisterNameOp {
        use crate::core::registry::RegisterNameOp;
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let r_in = Scalar::random(&mut rng);
        let r_change = Scalar::random(&mut rng);
        let input_value = 1000 + fee;
        let change_value = 1000;

        let input = Input { commitment: Commitment::new(input_value, r_in) };
        let change_commitment = Commitment::new(change_value, r_change);
        let change_proof = RangeProof::prove(change_value, &r_change);
        let change_output = Output { commitment: change_commitment, proof: change_proof, note: vec![] };

        let excess_blinding = r_in - r_change;
        let excess = Commitment::new(0, excess_blinding);
        let signature = Signature::sign(&fee.to_le_bytes(), &excess_blinding);
        let fee_payment = Transaction {
            inputs: vec![input],
            outputs: vec![change_output],
            kernels: vec![TxKernel { excess, fee, signature }],
        };

        let gens = PedersenGens::default();
        let owner_secret = Scalar::random(&mut rng);
        let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
        let signature = RegisterNameOp::sign(name, &owner_secret);

        RegisterNameOp { name: name.to_string(), owner_pubkey, resolves_to: owner_pubkey, fee_payment, signature }
    }

    #[test]
    fn add_name_op_rejects_below_registration_floor() {
        let mut mempool = Mempool::new();
        assert!(!mempool.add_name_op(make_valid_name_op_with_fee("toolow", NAME_REGISTRATION_FEE - 1)));
        assert_eq!(mempool.name_ops_len(), 0);
    }

    #[test]
    fn add_name_op_accepts_floor_and_above() {
        let mut mempool = Mempool::new();
        assert!(mempool.add_name_op(make_valid_name_op_with_fee("atfloor", NAME_REGISTRATION_FEE)));
        assert!(mempool.add_name_op(make_valid_name_op_with_fee("abovefloor", NAME_REGISTRATION_FEE * 3)));
        assert_eq!(mempool.name_ops_len(), 2);
    }

    #[test]
    fn take_name_ops_prioritizes_highest_fee_and_leaves_the_rest() {
        let mut mempool = Mempool::new();
        let total = MAX_NAME_OPS_PER_BLOCK + 3;
        for i in 0..total {
            mempool.add_name_op(make_valid_name_op_with_fee(&format!("name{}", i), NAME_REGISTRATION_FEE + i as u64));
        }
        assert_eq!(mempool.name_ops_len(), total);

        let taken = mempool.take_name_ops();
        assert_eq!(taken.len(), MAX_NAME_OPS_PER_BLOCK);
        assert_eq!(mempool.name_ops_len(), 3, "the 3 lowest-fee registrations should remain queued");

        let min_fee_taken: u64 = taken.iter().map(|op| op.fee_payment.kernels[0].fee).min().unwrap();
        assert!(min_fee_taken > NAME_REGISTRATION_FEE + 2);
    }

    #[test]
    fn suggested_name_fee_rises_with_backlog_and_floors_at_registration_fee() {
        let mut mempool = Mempool::new();
        assert_eq!(mempool.suggested_name_fee(), NAME_REGISTRATION_FEE, "empty backlog suggests the floor");

        for i in 0..MAX_NAME_OPS_PER_BLOCK {
            mempool.add_name_op(make_valid_name_op_with_fee(&format!("full{}", i), NAME_REGISTRATION_FEE));
        }
        assert_eq!(mempool.suggested_name_fee(), NAME_REGISTRATION_FEE, "exactly one block's worth is still the floor");

        mempool.add_name_op(make_valid_name_op_with_fee("overflow", NAME_REGISTRATION_FEE));
        assert_eq!(mempool.suggested_name_fee(), NAME_REGISTRATION_FEE * 2, "one more than a full block bumps the suggestion");
    }

    /// Mirrors make_valid_name_op_with_fee, but wrapped as a MintAssetOp.
    fn make_valid_mint_op_with_fee(asset_id: &str, fee: u64) -> MintAssetOp {
        use bulletproofs::PedersenGens;

        let mut rng = OsRng;
        let r_in = Scalar::random(&mut rng);
        let r_change = Scalar::random(&mut rng);
        let input_value = 1000 + fee;
        let change_value = 1000;

        let input = Input { commitment: Commitment::new(input_value, r_in) };
        let change_commitment = Commitment::new(change_value, r_change);
        let change_proof = RangeProof::prove(change_value, &r_change);
        let change_output = Output { commitment: change_commitment, proof: change_proof, note: vec![] };

        let excess_blinding = r_in - r_change;
        let excess = Commitment::new(0, excess_blinding);
        let signature = Signature::sign(&fee.to_le_bytes(), &excess_blinding);
        let fee_payment = Transaction {
            inputs: vec![input],
            outputs: vec![change_output],
            kernels: vec![TxKernel { excess, fee, signature }],
        };

        let gens = PedersenGens::default();
        let owner_secret = Scalar::random(&mut rng);
        let owner_pubkey = Commitment(owner_secret * gens.B_blinding);
        let metadata_hash = [3u8; 32];
        let signature = MintAssetOp::sign(asset_id, &metadata_hash, &owner_secret);

        MintAssetOp { asset_id: asset_id.to_string(), owner_pubkey, metadata_hash, fee_payment, signature }
    }

    #[test]
    fn add_mint_op_rejects_below_mint_floor() {
        let mut mempool = Mempool::new();
        assert!(!mempool.add_mint_op(make_valid_mint_op_with_fee("toolow", ASSET_MINT_FEE - 1)));
        assert_eq!(mempool.mint_ops_len(), 0);
    }

    #[test]
    fn add_mint_op_accepts_floor_and_above() {
        let mut mempool = Mempool::new();
        assert!(mempool.add_mint_op(make_valid_mint_op_with_fee("atfloor", ASSET_MINT_FEE)));
        assert!(mempool.add_mint_op(make_valid_mint_op_with_fee("abovefloor", ASSET_MINT_FEE * 3)));
        assert_eq!(mempool.mint_ops_len(), 2);
    }

    #[test]
    fn take_mint_ops_prioritizes_highest_fee_and_leaves_the_rest() {
        let mut mempool = Mempool::new();
        let total = MAX_MINT_OPS_PER_BLOCK + 3;
        for i in 0..total {
            mempool.add_mint_op(make_valid_mint_op_with_fee(&format!("asset{}", i), ASSET_MINT_FEE + i as u64));
        }
        assert_eq!(mempool.mint_ops_len(), total);

        let taken = mempool.take_mint_ops();
        assert_eq!(taken.len(), MAX_MINT_OPS_PER_BLOCK);
        assert_eq!(mempool.mint_ops_len(), 3, "the 3 lowest-fee mints should remain queued");

        let min_fee_taken: u64 = taken.iter().map(|op| op.fee_payment.kernels[0].fee).min().unwrap();
        assert!(min_fee_taken > ASSET_MINT_FEE + 2);
    }

    #[test]
    fn suggested_asset_fee_rises_with_backlog_and_floors_at_mint_fee() {
        let mut mempool = Mempool::new();
        assert_eq!(mempool.suggested_asset_fee(), ASSET_MINT_FEE, "empty backlog suggests the floor");

        for i in 0..MAX_MINT_OPS_PER_BLOCK {
            mempool.add_mint_op(make_valid_mint_op_with_fee(&format!("full{}", i), ASSET_MINT_FEE));
        }
        assert_eq!(mempool.suggested_asset_fee(), ASSET_MINT_FEE, "exactly one block's worth is still the floor");

        mempool.add_mint_op(make_valid_mint_op_with_fee("overflow", ASSET_MINT_FEE));
        assert_eq!(mempool.suggested_asset_fee(), ASSET_MINT_FEE * 2, "one more than a full block bumps the suggestion");
    }
}
