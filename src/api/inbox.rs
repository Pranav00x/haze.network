//! A minimal message relay so two wallets can complete the two-party slate
//! protocol (wallet::slate) automatically instead of manually copy-pasting
//! JSON between them - the sender resolves a recipient's registered name
//! (core::registry) to a pubkey, drops a slate off addressed to it, and the
//! recipient's wallet polls its own inbox and responds.
//!
//! Deliberately NOT part of consensus/ChainState: this is transport, not
//! chain state - messages are ephemeral, in-memory only, and lost on node
//! restart (same tier as a normal mailbox/notification queue, not something
//! that needs to survive or be agreed on by every node).
//!
//! Both directions are authenticated with the wallet's identity key (the
//! same Signature primitive/scheme used everywhere else in this codebase -
//! see crypto::schnorr, and wasm::sign_identity_message/
//! verify_identity_signature for the client-side helper this reuses):
//! - POST requires a signature proving the poster genuinely controls
//!   `from_pubkey_hex`, binding the exact (to, kind, payload) it's attached
//!   to - otherwise anyone could inject a message claiming to be from
//!   someone else (e.g. a fake "response" impersonating a marketplace
//!   seller, redirecting a buyer's payment to an attacker's own output -
//!   the interactive slate protocol itself has no recipient authentication
//!   beyond "whoever the transport actually delivered the request to", so
//!   this transport-level check is the only thing standing between that and
//!   real fund theft).
//! - GET requires a signature proving the poller genuinely controls the
//!   pubkey they're draining, over a fresh timestamp (bounded window) -
//!   otherwise anyone who merely knows a pubkey (they're public - published
//!   in the name registry, marketplace listings, etc.) could read and
//!   permanently drain someone else's inbox before they see it.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use crate::crypto::pedersen::Commitment;
use crate::crypto::schnorr::Signature;

/// A real inbox never needs more than a handful of in-flight handshake
/// messages per pubkey; this just bounds the cost of an authenticated
/// spammer (someone who legitimately owns a pubkey but posts to it
/// endlessly) rather than defending against forgery, which the signature
/// check above already rules out.
const MAX_MESSAGES_PER_PUBKEY: usize = 500;

/// How far a GET poll's signed timestamp may drift from the server's clock
/// before being rejected - bounds how long a captured/logged poll URL
/// remains replayable (replaying it only ever returns whatever's still
/// queued, so the actual risk is low, but there's no reason to allow it
/// indefinitely).
const POLL_TIMESTAMP_WINDOW_SECS: u64 = 120;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InboxMessage {
    /// Sender's identity pubkey (hex) - so the recipient knows who to send
    /// a response back to. Authenticated - see inbox_post_signing_message.
    pub from_pubkey_hex: String,
    /// "request" (an outgoing payment slate, awaiting a response) or
    /// "response" (a filled-in slate being sent back to whoever paid).
    pub kind: String,
    /// The Slate/response JSON itself (opaque to this relay).
    pub payload_json: String,
    /// Proves from_pubkey_hex genuinely authored this exact
    /// (to_pubkey_hex, kind, payload_json) - see inbox_post_signing_message.
    /// Hex-encoded (matches every other *_hex signature field in this
    /// codebase's JSON wire format).
    pub signature_hex: String,
}

#[derive(Deserialize)]
pub struct InboxPollQuery {
    pub timestamp: u64,
    pub signature_hex: String,
}

/// Binds a POST to one specific (to, kind, payload) tuple, so a captured
/// signature can't be replayed to redirect the message to a different
/// recipient or with tampered content. from_pubkey_hex itself isn't part of
/// the message - it's what the signature is verified AGAINST (the claimed
/// public key), not something bound *into* it.
pub fn inbox_post_signing_message(to_pubkey_hex: &str, kind: &str, payload_json: &str) -> Vec<u8> {
    format!("HazeInboxMessage:{}:{}:{}", to_pubkey_hex, kind, payload_json).into_bytes()
}

/// Binds a poll to one specific pubkey and a fresh timestamp - see
/// POLL_TIMESTAMP_WINDOW_SECS.
pub fn inbox_poll_signing_message(pubkey_hex: &str, timestamp: u64) -> Vec<u8> {
    format!("HazeInboxPoll:{}:{}", pubkey_hex, timestamp).into_bytes()
}

#[derive(Default)]
pub struct InboxState {
    // recipient pubkey hex -> queued messages addressed to them.
    messages: Mutex<HashMap<String, Vec<InboxMessage>>>,
}

impl InboxState {
    pub fn new() -> Self {
        Self::default()
    }
}

fn error_reply(status: warp::http::StatusCode, message: &str) -> Box<dyn warp::Reply> {
    Box::new(warp::reply::with_status(
        warp::reply::json(&serde_json::json!({ "status": "error", "message": message })),
        status,
    ))
}

pub async fn handle_post_inbox(
    to_pubkey_hex: String,
    message: InboxMessage,
    inbox: Arc<InboxState>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let Some(from_pubkey) = Commitment::from_hex(&message.from_pubkey_hex) else {
        return Ok(error_reply(warp::http::StatusCode::BAD_REQUEST, "invalid from_pubkey_hex"));
    };
    let Some(signature) = Signature::from_hex(&message.signature_hex) else {
        return Ok(error_reply(warp::http::StatusCode::BAD_REQUEST, "invalid signature_hex"));
    };
    let msg = inbox_post_signing_message(&to_pubkey_hex, &message.kind, &message.payload_json);
    if !signature.verify(&msg, &from_pubkey) {
        return Ok(error_reply(warp::http::StatusCode::UNAUTHORIZED, "signature does not match from_pubkey_hex for this message"));
    }

    let mut messages = inbox.messages.lock().unwrap();
    let queue = messages.entry(to_pubkey_hex).or_default();
    if queue.len() >= MAX_MESSAGES_PER_PUBKEY {
        return Ok(error_reply(warp::http::StatusCode::TOO_MANY_REQUESTS, "recipient inbox is full"));
    }
    queue.push(message);
    Ok(Box::new(warp::reply::json(&serde_json::json!({ "status": "delivered" }))))
}

/// Drains and returns all messages queued for this pubkey (poll-and-consume -
/// simplest correct semantics for a devnet-scale relay with no persistence).
/// Requires the caller to prove ownership of `pubkey_hex` - see module doc.
pub async fn handle_get_inbox(
    pubkey_hex: String,
    query: InboxPollQuery,
    inbox: Arc<InboxState>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let Some(pubkey) = Commitment::from_hex(&pubkey_hex) else {
        return Ok(error_reply(warp::http::StatusCode::BAD_REQUEST, "invalid pubkey_hex"));
    };
    let Some(signature) = Signature::from_hex(&query.signature_hex) else {
        return Ok(error_reply(warp::http::StatusCode::BAD_REQUEST, "invalid signature_hex"));
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let drift = now.max(query.timestamp) - now.min(query.timestamp);
    if drift > POLL_TIMESTAMP_WINDOW_SECS {
        return Ok(error_reply(warp::http::StatusCode::UNAUTHORIZED, "poll timestamp too far from server time"));
    }
    let msg = inbox_poll_signing_message(&pubkey_hex, query.timestamp);
    if !signature.verify(&msg, &pubkey) {
        return Ok(error_reply(warp::http::StatusCode::UNAUTHORIZED, "signature does not prove ownership of pubkey_hex"));
    }

    let mut messages = inbox.messages.lock().unwrap();
    let drained = messages.remove(&pubkey_hex).unwrap_or_default();
    Ok(Box::new(warp::reply::json(&drained)))
}
