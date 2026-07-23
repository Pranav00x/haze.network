//! Per-IP request throttling for the public API. `server.rs` already caps
//! request *body size*, but had nothing capping request *rate* - a caller
//! could otherwise flood any GET/POST route (block/UTXO scans, transaction
//! submission, etc.) with no cost to them. This is a plain sliding-window
//! counter, not a token bucket - simple enough to audit at a glance, and
//! sufficient to blunt casual flooding without needing a new dependency.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use warp::Filter;
use haze_chain::sync::LockExt;

/// Generous enough that no legitimate wallet/explorer polling loop (this
/// codebase polls /v1/status etc. every few seconds) ever comes close, but
/// low enough to blunt a naive flood from a single source.
const MAX_REQUESTS_PER_WINDOW: usize = 120;
const WINDOW: Duration = Duration::from_secs(10);

pub struct RateLimiter {
    hits: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self { hits: Mutex::new(HashMap::new()) }
    }

    /// Records a hit for `ip` and returns whether it's still within budget.
    /// Also periodically drops entries for IPs that have gone quiet, so this
    /// map doesn't grow forever under a rotating flood of source IPs.
    fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock_recover();

        if hits.len() > 10_000 {
            hits.retain(|_, times| times.last().is_some_and(|t| now.duration_since(*t) < WINDOW));
        }

        let times = hits.entry(ip).or_default();
        times.retain(|t| now.duration_since(*t) < WINDOW);

        if times.len() >= MAX_REQUESTS_PER_WINDOW {
            return false;
        }
        times.push(now);
        true
    }
}

impl Default for RateLimiter {
    fn default() -> Self { Self::new() }
}

#[derive(Debug)]
struct TooManyRequests;
impl warp::reject::Reject for TooManyRequests {}

/// A filter that rejects with 429 once `limiter` says an IP is over budget,
/// and otherwise passes the request through unchanged. Chain it in front of
/// the combined route tree with `.and()` so every route is covered.
pub fn guard(limiter: std::sync::Arc<RateLimiter>) -> impl Filter<Extract = (), Error = warp::Rejection> + Clone {
    warp::addr::remote()
        .and_then(move |addr: Option<std::net::SocketAddr>| {
            let limiter = limiter.clone();
            async move {
                let allowed = match addr {
                    // No peer address at all (e.g. a raw unix-socket test
                    // client) - fail open rather than break every existing
                    // caller that doesn't go through a real TCP connection.
                    None => true,
                    Some(a) => limiter.check(a.ip()),
                };
                if allowed { Ok(()) } else { Err(warp::reject::custom(TooManyRequests)) }
            }
        })
        .untuple_one()
}

/// Only handles our own rejection and passes everything else through
/// unchanged, so this doesn't shadow warp's normal 404/other handling for
/// routes that were never ours to begin with.
pub async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, warp::Rejection> {
    if err.find::<TooManyRequests>().is_some() {
        Ok(warp::reply::with_status("Too many requests", warp::http::StatusCode::TOO_MANY_REQUESTS))
    } else {
        Err(err)
    }
}
