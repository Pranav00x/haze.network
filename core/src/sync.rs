//! Poison-tolerant locking for the long-running node.
//!
//! A std `Mutex` becomes *poisoned* if a thread panics while holding its
//! guard. Once poisoned, every subsequent `.lock().unwrap()` on that same
//! mutex also panics. On a public node this is the difference between a
//! recoverable hiccup and permanent death: warp/tokio already isolate a
//! panic to the single request/task it happened in, but if that task was
//! holding a shared lock (the chain state, the mempool), the poison then
//! makes *every* future request that touches that lock panic too - one
//! bad input silently bricks the whole node while the process stays alive.
//!
//! `lock_recover()` takes the guard back even from a poisoned mutex, so the
//! node keeps serving. The tradeoff is that a panic *mid-mutation* could
//! leave in-memory state partially updated - but in practice the panics
//! worth surviving are on read/serialize paths where state is untouched,
//! any real drift is re-checked against consensus rules before it can
//! affect a block, and the whole in-memory state is rebuilt from the
//! on-disk block log on the next restart. For a network whose entire point
//! is staying reachable, availability wins over strict in-memory
//! consistency here.

use std::sync::{Mutex, MutexGuard, PoisonError};

pub trait LockExt<T> {
    /// Like `lock().unwrap()`, but returns the guard even if the mutex was
    /// poisoned by a panic in another thread instead of panicking itself.
    /// See the module docs for why the node prefers this everywhere.
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
