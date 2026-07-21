//! Repeatable devnet faucet, distinct from the wallet's single-use
//! claim-genesis convenience. Funded by the treasury genesis allocation
//! (see core::genesis::TREASURY_OUTPUT) that only this node's own embedded
//! wallet identity ever spends from - every request runs the same
//! two-party slate protocol (wallet::slate) the web wallet already uses
//! for peer-to-peer payments, just with this node playing the sender.
//! Unlike earlier revisions, the treasury secret is supplied at runtime via
//! HAZE_TREASURY_BLINDING (see wallet::planner::treasury_blinding_from_env)
//! rather than committed to source.
use std::sync::Mutex;
use haze_chain::sync::LockExt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use warp::http::StatusCode;

use haze_chain::chain::ChainState;
use haze_chain::genesis::TREASURY_ALLOCATION;
use haze_chain::mempool::Mempool;
use haze_chain::transaction::{Transaction, Input, Output, TxKernel};
use haze_crypto::pedersen::Commitment;
use haze_crypto::range_proof::RangeProof;
use haze_crypto::schnorr::Signature;
use haze_wallet::keystore::Keystore;
use haze_wallet::store::{WalletStore, OutputStatus, FAUCET_INDEX};
use haze_wallet::slate::{self, PendingSlate, Slate};
use haze_wallet::planner::{self, PlanError};
use haze_wallet::recovery::{self, ScanEntry};
use curve25519_dalek_ng::scalar::Scalar;
use sha2::{Sha512, Digest};

/// Devnet-only cap per request - keeps a single requester from draining the
/// reserve, not a real anti-abuse measure.
const MAX_FAUCET_AMOUNT: u64 = 1000;

/// How long a pending faucet request is held before it's treated as
/// abandoned and released - without this, a caller who requests a slate
/// and never completes it (network drop, closed tab, or just testing the
/// endpoint) would permanently lock every other requester out until the
/// node restarts. Generous relative to how long an honest two-party
/// round-trip actually takes (seconds), since this is only a safety net.
const PENDING_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-IP rate limit: at most this many /v1/faucet requests within
/// RATE_LIMIT_WINDOW. Devnet-scale anti-abuse, same tier as
/// MAX_FAUCET_AMOUNT - not meant to withstand a determined attacker with
/// many IPs, just to stop a single careless script (or accidental retry
/// loop) from draining the reserve or spamming the mempool.
const RATE_LIMIT_MAX_REQUESTS: usize = 5;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(3600);

pub struct FaucetState {
    keystore: Mutex<Keystore>,
    store: Mutex<WalletStore>,
    /// Only one faucet payout in flight at a time - simpler than juggling
    /// concurrent PendingSlates, and faucet requests aren't latency-sensitive.
    /// Paired with when it was created so an abandoned request can be
    /// released after PENDING_TIMEOUT instead of locking the faucet forever.
    pending: Mutex<Option<(PendingSlate, Instant)>>,
    /// False when HAZE_TREASURY_BLINDING wasn't set (or didn't match the
    /// real genesis treasury output) at startup - the rest of the API keeps
    /// running normally, only /v1/faucet is unavailable (see
    /// handle_faucet_request).
    enabled: bool,
    /// Sliding-window request log per client IP (see RATE_LIMIT_MAX_REQUESTS).
    rate_limits: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

/// Derives a stable seed for the faucet's internal keystore from the
/// treasury secret itself, rather than a fresh random one every process
/// restart (see FaucetState::new's doc comment for why this matters - it's
/// what makes the faucet's own change outputs recoverable across restarts
/// instead of permanently stranded the moment the process cycles).
fn faucet_keystore_seed(secret: &Scalar) -> [u8; 32] {
    let mut hasher = Sha512::new();
    hasher.update(b"Haze Faucet Keystore Seed");
    hasher.update(secret.as_bytes());
    let result = hasher.finalize();
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&result[0..32]);
    seed
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Scans `chain` for anything sealed under `keystore`'s own note_key (see
/// wallet::note) and merges any real, currently-unspent match into `store`
/// as Confirmed - the actual self-recovery mechanism, factored out so it's
/// testable independently of FaucetState::new's real-treasury-secret gate
/// (which can't be exercised in tests - the real secret is intentionally
/// out-of-repo, see core::genesis's module doc).
fn recover_faucet_change(keystore: &mut Keystore, store: &mut WalletStore, chain: &ChainState) {
    let (blocks, _) = chain.get_blocks_from(0, usize::MAX);
    let scan_entries: Vec<ScanEntry> = blocks.iter()
        .flat_map(super::explorer::all_outputs)
        .filter(|o| !o.note.is_empty())
        .map(|o| ScanEntry { commitment_hex: o.commitment.to_hex(), note_hex: bytes_to_hex(&o.note) })
        .collect();
    let utxo_hex: HashSet<String> = chain.utxos.iter().map(|c| c.to_hex()).collect();
    let recovered = recovery::recover_from_chain(keystore, &scan_entries, &utxo_hex);
    if recovered.recovered_count > 0 {
        println!("Faucet recovered {} past change output(s) totaling {} from a previous process lifetime.", recovered.recovered_count, recovered.recovered_balance);
    }
    for output in recovered.store.spendable() {
        store.add_output(output.index, output.value, output.commitment, OutputStatus::Confirmed);
    }
}

impl FaucetState {
    /// `chain` is a snapshot of chain state at startup, used to recover the
    /// faucet's true current balance rather than assuming only the original
    /// genesis treasury allocation still exists (see the self-recovery scan
    /// below) - critical now that the faucet's identity is stable across
    /// restarts: without this, a fresh process would only ever know about
    /// its very first output, ignoring any real change from registrations
    /// sponsored in a previous process lifetime.
    pub fn new(chain: &ChainState) -> Self {
        let secret = match planner::treasury_blinding_from_env() {
            Some(s) => s,
            None => {
                println!("Note: HAZE_TREASURY_BLINDING not set - devnet faucet disabled on this node (everything else runs normally).");
                let keystore = Keystore::generate();
                return Self { keystore: Mutex::new(keystore), store: Mutex::new(WalletStore::default()), pending: Mutex::new(None), enabled: false, rate_limits: Mutex::new(HashMap::new()) };
            }
        };

        // Stable across restarts, unlike the old Keystore::generate() - see
        // faucet_keystore_seed's doc comment.
        let mut keystore = Keystore::from_seed(faucet_keystore_seed(&secret));

        let commitment = Commitment::new(TREASURY_ALLOCATION, secret);
        if commitment != haze_chain::genesis::TREASURY_OUTPUT.commitment() {
            println!("Warning: HAZE_TREASURY_BLINDING does not match the real genesis treasury output - devnet faucet disabled on this node.");
            return Self { keystore: Mutex::new(keystore), store: Mutex::new(WalletStore::default()), pending: Mutex::new(None), enabled: false, rate_limits: Mutex::new(HashMap::new()) };
        }

        // The one genesis treasury output itself has no note (see
        // core::genesis::build_locked_output) - it's never discoverable via
        // chain-scan, so it stays a special case, seeded unconditionally.
        let mut store = WalletStore::default();
        store.add_output(FAUCET_INDEX, TREASURY_ALLOCATION, commitment, OutputStatus::Confirmed);

        // Self-recovery: real change from a registration sponsored in a
        // previous process lifetime, however many restarts ago, is still
        // recoverable now that the keystore is stable (see
        // recover_faucet_change).
        recover_faucet_change(&mut keystore, &mut store, chain);

        // Now correct for real, current chain state (using the fixed
        // Spent->Confirmed reconciliation too) rather than trusting the
        // FAUCET_INDEX seed blindly - it may already be for-real spent.
        store.reconcile(&chain.utxos.iter().cloned().collect());

        Self {
            keystore: Mutex::new(keystore),
            store: Mutex::new(store),
            pending: Mutex::new(None),
            enabled: true,
            rate_limits: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn reconcile(&self, chain: &ChainState) {
        let utxos: HashSet<Commitment> = chain.utxos.iter().cloned().collect();
        self.store.lock_recover().reconcile(&utxos);
    }

    /// True (and records this request) if `ip` is still under
    /// RATE_LIMIT_MAX_REQUESTS within RATE_LIMIT_WINDOW; false if it should
    /// be rejected. Sliding window: prunes timestamps older than the window
    /// on every check rather than resetting on a fixed boundary, so a burst
    /// right at a window edge can't double an effective limit.
    fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let mut limits = self.rate_limits.lock_recover();
        let now = Instant::now();
        let entry = limits.entry(ip).or_default();
        while let Some(&oldest) = entry.front() {
            if now.duration_since(oldest) > RATE_LIMIT_WINDOW {
                entry.pop_front();
            } else {
                break;
            }
        }
        if entry.len() >= RATE_LIMIT_MAX_REQUESTS {
            return false;
        }
        entry.push_back(now);
        true
    }

    /// Builds a plain fee-paying transaction from the faucet's own reserve -
    /// no destination output, no two-party protocol needed, since this only
    /// ever sponsors someone ELSE's fee (see api/names.rs's sponsored
    /// registration), not a payment to them. This is what lets a brand-new
    /// wallet with zero balance still register a name: it signs the
    /// registration itself (free - just needs its own secret key), and the
    /// faucet covers the flat fee.
    /// Returns the built transaction alongside what's needed to undo this
    /// call's store mutations (the inputs it optimistically marked Spent,
    /// and the index of the Pending change output it created, if any) - see
    /// revert_fee_payment. Necessary because the caller queues `fee_payment`
    /// into the mempool as part of a larger op (see
    /// api::names::handle_sponsored_register_name) that can still fail
    /// *after* this call succeeds (e.g. a duplicate-registration race), at
    /// which point this spend never actually happens on-chain.
    pub fn build_sponsored_fee_payment(&self, fee: u64) -> Result<(Transaction, Vec<Commitment>, Option<u32>), PlanError> {
        let mut keystore = self.keystore.lock_recover();
        let mut store = self.store.lock_recover();

        let selected = planner::select_spendable_confirmed_only(&store, fee)?;
        let selected_total: u64 = selected.iter().map(|(_, _, v)| v).sum();

        let mut input_blindings: Vec<Scalar> = Vec::new();
        let mut inputs: Vec<Input> = Vec::new();
        let mut spent: Vec<Commitment> = Vec::new();
        for (index, commitment, _value) in &selected {
            input_blindings.push(planner::blinding_for(&keystore, *index));
            inputs.push(Input { commitment: *commitment });
            spent.push(*commitment);
        }

        let change_value = selected_total - fee;
        let (outputs, change_blinding, change_index) = if change_value > 0 {
            let change_index = keystore.allocate_index();
            let change_blinding = keystore.derive_blinding(change_index);
            let change_commitment = Commitment::new(change_value, change_blinding);
            let change_proof = RangeProof::prove(change_value, &change_blinding);
            let change_note = haze_crypto::note::seal(&keystore.note_key(), change_index, change_value);
            let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };
            store.add_output(change_index, change_value, change_commitment, OutputStatus::Pending);
            (vec![output], change_blinding, Some(change_index))
        } else {
            (vec![], Scalar::zero(), None)
        };

        for c in &spent {
            store.mark_spent(c);
        }

        let sum_input_blinding: Scalar = input_blindings.iter().sum();
        let excess_r = sum_input_blinding - change_blinding;
        let kernel = TxKernel {
            excess: Commitment::new(0, excess_r),
            fee,
            signature: Signature::sign(&fee.to_le_bytes(), &excess_r),
        };

        Ok((Transaction { inputs, outputs, kernels: vec![kernel] }, spent, change_index))
    }

    /// Undoes build_sponsored_fee_payment's store mutations for a fee
    /// payment that was built but never actually got queued (see
    /// api::names::handle_sponsored_register_name's `!added` branch) -
    /// without this, that spend is permanently and incorrectly lost from
    /// the faucet's local view of its own confirmed balance, since nothing
    /// else ever reverts it (reconcile() only confirms real changes, it
    /// doesn't undo a spend that never happened on-chain).
    pub fn revert_fee_payment(&self, spent: &[Commitment], change_index: Option<u32>) {
        let mut store = self.store.lock_recover();
        for c in spent {
            store.unmark_spent(c);
        }
        if let Some(index) = change_index {
            store.remove_pending_output(index);
        }
    }
}

#[derive(Deserialize)]
pub struct FaucetRequest {
    amount: u64,
}

#[derive(Serialize)]
struct FaucetSlateResponse {
    slate_json: String,
}

#[derive(Serialize)]
struct FaucetErrorResponse {
    error: String,
}

fn error_reply(status: StatusCode, message: impl Into<String>) -> Box<dyn warp::Reply> {
    Box::new(warp::reply::with_status(warp::reply::json(&FaucetErrorResponse { error: message.into() }), status))
}

/// The requester's real IP, preferring the leftmost X-Forwarded-For entry
/// (the original client, by convention) over the raw socket peer - this
/// node runs behind a reverse proxy in production (Render), so the socket
/// peer alone would just be the proxy's address for every request, making
/// per-IP rate limiting a no-op there.
fn client_ip(forwarded_for: Option<String>, remote: Option<std::net::SocketAddr>) -> Option<IpAddr> {
    if let Some(header) = forwarded_for {
        if let Some(first) = header.split(',').next() {
            if let Ok(ip) = first.trim().parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }
    remote.map(|addr| addr.ip())
}

pub async fn handle_faucet_request(
    req: FaucetRequest,
    forwarded_for: Option<String>,
    remote_addr: Option<std::net::SocketAddr>,
    faucet: std::sync::Arc<FaucetState>,
    chain: std::sync::Arc<Mutex<ChainState>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    if !faucet.enabled {
        return Ok(error_reply(StatusCode::SERVICE_UNAVAILABLE, "faucet is not configured on this node (HAZE_TREASURY_BLINDING unset)"));
    }

    if req.amount == 0 || req.amount > MAX_FAUCET_AMOUNT {
        return Ok(error_reply(StatusCode::BAD_REQUEST, format!("amount must be between 1 and {}", MAX_FAUCET_AMOUNT)));
    }

    match client_ip(forwarded_for, remote_addr) {
        Some(ip) => {
            if !faucet.check_rate_limit(ip) {
                return Ok(error_reply(
                    StatusCode::TOO_MANY_REQUESTS,
                    format!("rate limit exceeded - at most {} faucet requests per hour per IP", RATE_LIMIT_MAX_REQUESTS),
                ));
            }
        }
        // No IP could be determined at all (shouldn't happen for a real HTTP
        // connection) - fail closed rather than silently skipping the limit.
        None => return Ok(error_reply(StatusCode::BAD_REQUEST, "could not determine requester IP")),
    }

    {
        let c = chain.lock_recover();
        faucet.reconcile(&c);
    }

    let mut pending_guard = faucet.pending.lock_recover();
    if let Some((_, created_at)) = pending_guard.as_ref() {
        if created_at.elapsed() < PENDING_TIMEOUT {
            return Ok(error_reply(StatusCode::CONFLICT, "faucet is completing another request, try again in a few seconds"));
        }
        // Older than PENDING_TIMEOUT - treat as abandoned and fall through
        // to replace it with this new request.
    }

    let mut keystore = faucet.keystore.lock_recover();
    let store = faucet.store.lock_recover();

    // Pays the mempool's fee floor (see core::mempool::MIN_FEE) from the
    // faucet's own reserve, on top of req.amount - the requester still gets
    // the full amount they asked for, since plan_send/create_slate's fee is
    // additional to the destination output, not deducted from it. A flat 0
    // used to work fine here since nothing enforced a minimum; now that
    // add_transaction rejects anything under MIN_FEE, this transaction needs
    // a real fee to even enter a mempool.
    match slate::create_slate(&mut keystore, &store, req.amount, haze_chain::mempool::MIN_FEE) {
        Ok((built_slate, pending)) => {
            *pending_guard = Some((pending, Instant::now()));
            let slate_json = serde_json::to_string(&built_slate).unwrap();
            Ok(Box::new(warp::reply::json(&FaucetSlateResponse { slate_json })))
        }
        Err(PlanError::InsufficientBalance { .. }) => {
            Ok(error_reply(StatusCode::SERVICE_UNAVAILABLE, "faucet reserve temporarily depleted (recent payouts still confirming) - try again shortly"))
        }
    }
}

#[derive(Deserialize)]
pub struct FaucetCompleteRequest {
    response_slate_json: String,
}

#[derive(Serialize)]
struct FaucetCompleteResponse {
    status: String,
}

pub async fn handle_faucet_complete(
    req: FaucetCompleteRequest,
    faucet: std::sync::Arc<FaucetState>,
    mempool: std::sync::Arc<Mutex<Mempool>>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let pending = {
        let mut pending_guard = faucet.pending.lock_recover();
        match pending_guard.take() {
            Some((p, _)) => p,
            None => return Ok(error_reply(StatusCode::BAD_REQUEST, "no pending faucet request - call /v1/faucet first")),
        }
    };

    let response: Slate = match serde_json::from_str(&req.response_slate_json) {
        Ok(s) => s,
        Err(_) => return Ok(error_reply(StatusCode::BAD_REQUEST, "invalid response slate JSON")),
    };

    let transaction = match slate::finalize_slate(&pending, &response) {
        Ok(tx) => tx,
        Err(_) => return Ok(error_reply(StatusCode::BAD_REQUEST, "incomplete response slate")),
    };

    if !transaction.validate() {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "constructed faucet transaction failed validation"));
    }

    let added = {
        let mut mp = mempool.lock_recover();
        mp.add_transaction(transaction)
    };

    if !added {
        return Ok(error_reply(StatusCode::BAD_REQUEST, "mempool rejected the faucet transaction"));
    }

    // Applied optimistically (before mining), same convention as the web
    // wallet's own commit_send/commit_slate_send - avoids the reserve output
    // getting re-selected by a second request before this one confirms.
    let mut store = faucet.store.lock_recover();
    for commitment in &pending.spent_commitments {
        store.mark_spent(commitment);
    }
    if let Some(change) = &pending.change {
        store.add_output(change.index, change.value, change.output.commitment, OutputStatus::Pending);
    }

    Ok(Box::new(warp::reply::json(&FaucetCompleteResponse { status: "success".to_string() })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ip_prefers_leftmost_forwarded_for_entry() {
        let forwarded = Some("203.0.113.5, 10.0.0.1, 10.0.0.2".to_string());
        let remote = Some("127.0.0.1:12345".parse().unwrap());
        assert_eq!(client_ip(forwarded, remote), Some("203.0.113.5".parse().unwrap()));
    }

    #[test]
    fn client_ip_falls_back_to_remote_addr_when_header_absent() {
        let remote = Some("198.51.100.7:8332".parse().unwrap());
        assert_eq!(client_ip(None, remote), Some("198.51.100.7".parse().unwrap()));
    }

    #[test]
    fn client_ip_falls_back_to_remote_addr_when_header_unparseable() {
        let forwarded = Some("not-an-ip".to_string());
        let remote = Some("198.51.100.7:8332".parse().unwrap());
        assert_eq!(client_ip(forwarded, remote), Some("198.51.100.7".parse().unwrap()));
    }

    #[test]
    fn client_ip_is_none_when_nothing_available() {
        assert_eq!(client_ip(None, None), None);
    }

    #[test]
    fn rate_limit_allows_up_to_max_then_rejects() {
        let faucet = blank_faucet_state();
        let ip: IpAddr = "203.0.113.9".parse().unwrap();

        for _ in 0..RATE_LIMIT_MAX_REQUESTS {
            assert!(faucet.check_rate_limit(ip), "requests under the limit must be allowed");
        }
        assert!(!faucet.check_rate_limit(ip), "the request that exceeds the limit must be rejected");
    }

    #[test]
    fn rate_limit_is_tracked_independently_per_ip() {
        let faucet = blank_faucet_state();
        let ip_a: IpAddr = "203.0.113.1".parse().unwrap();
        let ip_b: IpAddr = "203.0.113.2".parse().unwrap();

        for _ in 0..RATE_LIMIT_MAX_REQUESTS {
            assert!(faucet.check_rate_limit(ip_a));
        }
        assert!(!faucet.check_rate_limit(ip_a), "ip_a must now be limited");
        assert!(faucet.check_rate_limit(ip_b), "a different IP must be unaffected by ip_a's limit");
    }

    /// A FaucetState with the faucet itself disabled (no HAZE_TREASURY_BLINDING
    /// needed) - these tests only exercise the rate limiter, not real payouts.
    fn blank_faucet_state() -> FaucetState {
        FaucetState {
            keystore: Mutex::new(Keystore::generate()),
            store: Mutex::new(WalletStore::default()),
            pending: Mutex::new(None),
            enabled: false,
            rate_limits: Mutex::new(HashMap::new()),
        }
    }

    #[test]
    fn faucet_keystore_seed_is_deterministic_and_domain_separated() {
        let secret_a = Scalar::from(111u64);
        let secret_b = Scalar::from(222u64);
        assert_eq!(faucet_keystore_seed(&secret_a), faucet_keystore_seed(&secret_a));
        assert_ne!(faucet_keystore_seed(&secret_a), faucet_keystore_seed(&secret_b));
        // Domain-separated from just using the raw secret bytes directly -
        // otherwise a leaked faucet keystore seed would double as the raw
        // treasury secret itself.
        assert_ne!(faucet_keystore_seed(&secret_a), secret_a.to_bytes());
    }

    /// The actual regression this session's live-bug investigation traced:
    /// real change from a registration sponsored in a previous process
    /// lifetime must still be recoverable now that the faucet's keystore is
    /// stable (Keystore::from_seed, not Keystore::generate()) - this is
    /// what recover_faucet_change proves, independent of FaucetState::new's
    /// real-treasury-secret gate (which can't be exercised in a test - the
    /// real secret is intentionally out-of-repo).
    #[test]
    fn recover_faucet_change_finds_real_past_change_after_a_simulated_restart() {
        use haze_chain::block::{Block, BlockHeader};
        use haze_chain::transaction::{Input, Output, TxKernel};
        use haze_crypto::range_proof::RangeProof;
        use haze_crypto::schnorr::Signature;

        let mut chain = ChainState::new();
        let genesis = haze_chain::genesis::genesis_block();
        assert!(chain.apply_block(&genesis).is_applied());

        // Simulate "a previous faucet process lifetime already sponsored a
        // registration": spend the well-known genesis validator/claim
        // output (blinding=42) into one fresh output owned by a fake
        // faucet keystore, sealed with a real note the way
        // build_sponsored_fee_payment actually does.
        let fake_secret = Scalar::from(424242u64);
        let mut fake_keystore = Keystore::from_seed(faucet_keystore_seed(&fake_secret));
        let change_index = fake_keystore.allocate_index();
        // Must equal spent-input-value + this height's block reward for the
        // block to balance (Transaction::validate_with_reward) - genesis's
        // well-known claim/stake output (1,000,000) plus block_reward_at(1).
        let change_value = 1_000_000u64 + haze_chain::block::block_reward_at(1);
        let change_blinding = fake_keystore.derive_blinding(change_index);
        let change_commitment = Commitment::new(change_value, change_blinding);
        let change_proof = RangeProof::prove(change_value, &change_blinding);
        let change_note = haze_crypto::note::seal(&fake_keystore.note_key(), change_index, change_value);
        let output = Output { commitment: change_commitment, proof: change_proof, note: change_note };

        let genesis_secret = Scalar::from(42u64);
        let excess_r = genesis_secret - change_blinding;
        let kernel = TxKernel {
            excess: Commitment::new(0, excess_r),
            fee: 0,
            signature: Signature::sign(&0u64.to_le_bytes(), &excess_r),
        };

        let private_key = Scalar::from(42u64);
        let mut header = BlockHeader {
            height: 1,
            prev_hash: genesis.header.hash(),
            total_kernel_offset: Scalar::zero(),
            nonce: 0,
            timestamp: 0,
            validator_commitment: Commitment::new(1_000_000, private_key),
            validator_signature: Signature { s: Scalar::zero(), e: Scalar::zero() },
            name_registry_root: haze_chain::registry::compute_registry_root(&std::collections::HashMap::new()),
            chain_id: haze_chain::genesis::CHAIN_ID,
            asset_registry_root: haze_chain::assets::compute_asset_registry_root(&std::collections::HashMap::new()),
            collection_registry_root: haze_chain::collections::compute_collection_registry_root(&std::collections::HashMap::new()),
        };
        let msg = header.hash();
        header.validator_signature = Signature::sign(&msg, &private_key);

        let block = Block {
            header,
            body: haze_chain::transaction::Transaction {
                inputs: vec![Input { commitment: Commitment::new(1_000_000, genesis_secret) }],
                outputs: vec![output],
                kernels: vec![kernel],
            },
            name_ops: vec![],
            transfer_ops: vec![],
            mint_ops: vec![],
            transfer_asset_ops: vec![],
            launch_collection_ops: vec![],
            validator_ops: vec![],
        };
        assert!(chain.apply_block(&block).is_applied(), "test block must apply cleanly");

        // Simulate the process restart: a brand new keystore built the same
        // (stable) way, with no memory of change_index, and an empty store.
        let mut restarted_keystore = Keystore::from_seed(faucet_keystore_seed(&fake_secret));
        let mut store = WalletStore::default();

        recover_faucet_change(&mut restarted_keystore, &mut store, &chain);

        assert_eq!(store.balance(), change_value, "must recover the real past change output after a simulated restart");
    }
}
