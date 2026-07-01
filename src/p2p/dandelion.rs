use rand::Rng;

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
}

impl DandelionRouter {
    pub fn new(fluff_probability: f64) -> Self {
        Self { fluff_probability }
    }

    /// Determines the next state for a transaction currently in the Stem phase
    pub fn next_state(&self) -> TxState {
        let mut rng = rand::rng();
        if rng.random_bool(self.fluff_probability) {
            TxState::Fluff
        } else {
            TxState::Stem
        }
    }
}

// In a real implementation using Tokio, this would look like:
// pub async fn run_dandelion_node() {
//    let router = DandelionRouter::new(0.10); // 10% chance to fluff
//    loop {
//        // wait for incoming tx
//        // if tx is Stem:
//        //   match router.next_state() {
//        //     Stem => send to exactly one random peer (using tokio::net::TcpStream)
//        //     Fluff => broadcast to all peers
//        //   }
//    }
// }
