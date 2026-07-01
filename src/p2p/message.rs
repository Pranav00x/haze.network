use serde::{Serialize, Deserialize};
use crate::core::transaction::Transaction;
use crate::core::block::Block;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum P2pMessage {
    Handshake { listen_addr: String },
    Ping,
    Pong,
    StemTx(Transaction),
    FluffTx(Transaction),
    NewBlock(Block),
}
