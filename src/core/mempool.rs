use super::transaction::Transaction;
use super::cut_through::aggregate_and_cut_through;
use super::registry::RegisterNameOp;

/// Caps how many name registrations can land in a single block, bounding
/// block size the same way SYNC_BATCH_SIZE bounds a sync batch elsewhere.
pub const MAX_NAME_OPS_PER_BLOCK: usize = 10;

pub struct Mempool {
    pending_txs: Vec<Transaction>,
    pending_name_ops: Vec<RegisterNameOp>,
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            pending_txs: Vec::new(),
            pending_name_ops: Vec::new(),
        }
    }

    /// Validates and adds a transaction to the mempool
    pub fn add_transaction(&mut self, tx: Transaction) -> bool {
        if tx.validate() {
            self.pending_txs.push(tx);
            true
        } else {
            false
        }
    }

    /// Aggregates all pending transactions and performs cut-through
    pub fn aggregate(&mut self) -> Option<Transaction> {
        if self.pending_txs.is_empty() {
            return None;
        }

        let txs = std::mem::take(&mut self.pending_txs);
        Some(aggregate_and_cut_through(txs))
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
    /// the next block.
    pub fn take_name_ops(&mut self) -> Vec<RegisterNameOp> {
        let n = self.pending_name_ops.len().min(MAX_NAME_OPS_PER_BLOCK);
        self.pending_name_ops.drain(0..n).collect()
    }

    pub fn name_ops_len(&self) -> usize {
        self.pending_name_ops.len()
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
}
