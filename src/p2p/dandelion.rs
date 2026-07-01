use rand::Rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use crate::core::transaction::Transaction;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxState {
    /// The transaction is in the Stem phase (routing to exactly one peer)
    Stem,
    /// The transaction is in the Fluff phase (gossip broadcast to all peers)
    Fluff,
}

pub struct DandelionRouter {
    /// The probability of transitioning from Stem to Fluff
    pub fluff_probability: f64,
    /// Tracks whether a transaction ID (hash) has been fluffed
    seen_stems: Arc<Mutex<HashMap<[u8; 32], bool>>>,
}

impl DandelionRouter {
    pub fn new(fluff_probability: f64) -> Self {
        Self {
            fluff_probability,
            seen_stems: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Determines the next state for a transaction currently in the Stem phase
    pub fn next_state(&self) -> TxState {
        let mut rng = rand::thread_rng();
        if rng.gen_bool(self.fluff_probability) {
            TxState::Fluff
        } else {
            TxState::Stem
        }
    }

    /// Registers a stem transaction and schedules a fallback fluff timeout.
    /// If the transaction does not transition to fluff before the timeout, on_timeout is called.
    pub fn register_stem_tx<F>(&self, tx_id: [u8; 32], timeout_secs: u64, on_timeout: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let mut seen = self.seen_stems.lock().unwrap();
        if seen.contains_key(&tx_id) {
            return; // Already tracked
        }
        seen.insert(tx_id, false);

        let seen_clone = Arc::clone(&self.seen_stems);
        tokio::spawn(async move {
            sleep(Duration::from_secs(timeout_secs)).await;
            
            let trigger_fluff = {
                let mut seen_lock = seen_clone.lock().unwrap();
                if let Some(fluffed) = seen_lock.get(&tx_id) {
                    if !*fluffed {
                        seen_lock.insert(tx_id, true);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if trigger_fluff {
                on_timeout();
            }
        });
    }

    /// Marks a transaction as fluffed (either because we gossiped it or saw it fluff on the network)
    pub fn mark_fluffed(&self, tx_id: [u8; 32]) {
        let mut seen = self.seen_stems.lock().unwrap();
        seen.insert(tx_id, true);
    }

    /// Checks if we've already marked this transaction as fluffed
    pub fn is_fluffed(&self, tx_id: [u8; 32]) -> bool {
        let seen = self.seen_stems.lock().unwrap();
        seen.get(&tx_id).copied().unwrap_or(false)
    }
}

/// Helper to compute a stable transaction ID
pub fn compute_tx_id(tx: &Transaction) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let bytes = bincode::serialize(tx).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    let mut tx_id = [0u8; 32];
    tx_id.copy_from_slice(&result);
    tx_id
}
