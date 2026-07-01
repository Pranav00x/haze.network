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
}
