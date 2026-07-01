use super::transaction::Transaction;
use super::cut_through::aggregate_and_cut_through;

pub struct Mempool {
    pending_txs: Vec<Transaction>,
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            pending_txs: Vec::new(),
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
}
