//! A minimal message relay so two wallets can complete the two-party slate
//! protocol (wallet::slate) automatically instead of manually copy-pasting
//! JSON between them - the sender resolves a recipient's registered name
//! (core::registry) to a pubkey, drops a slate off addressed to it, and the
//! recipient's wallet polls its own inbox and responds.
//!
//! Deliberately NOT part of consensus/ChainState: this is transport, not
//! chain state - messages are ephemeral, in-memory only, and lost on node
//! restart (same tier as a normal mailbox/notification queue, not something
//! that needs to survive or be agreed on by every node). Anyone with the
//! recipient's pubkey can currently drop mail in their box (no anti-spam) -
//! fine for a devnet, would need hardening for anything more.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InboxMessage {
    /// Sender's identity pubkey (hex) - so the recipient knows who to send
    /// a response back to.
    pub from_pubkey_hex: String,
    /// "request" (an outgoing payment slate, awaiting a response) or
    /// "response" (a filled-in slate being sent back to whoever paid).
    pub kind: String,
    /// The Slate/response JSON itself (opaque to this relay).
    pub payload_json: String,
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

pub async fn handle_post_inbox(
    to_pubkey_hex: String,
    message: InboxMessage,
    inbox: Arc<InboxState>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let mut messages = inbox.messages.lock().unwrap();
    messages.entry(to_pubkey_hex).or_default().push(message);
    Ok(Box::new(warp::reply::json(&serde_json::json!({ "status": "delivered" }))))
}

/// Drains and returns all messages queued for this pubkey (poll-and-consume -
/// simplest correct semantics for a devnet-scale relay with no persistence).
pub async fn handle_get_inbox(
    pubkey_hex: String,
    inbox: Arc<InboxState>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    let mut messages = inbox.messages.lock().unwrap();
    let drained = messages.remove(&pubkey_hex).unwrap_or_default();
    Ok(Box::new(warp::reply::json(&drained)))
}
