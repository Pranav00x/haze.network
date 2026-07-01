use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use super::chain::ChainState;

const STATE_FILE: &str = "haze_data/chain_state.dat";

pub struct Storage;

impl Storage {
    pub fn init() {
        if !Path::new("haze_data").exists() {
            fs::create_dir("haze_data").unwrap();
        }
    }

    pub fn save_state(state: &ChainState) -> std::io::Result<()> {
        let encoded = bincode::serialize(state).unwrap();
        let mut file = File::create(STATE_FILE)?;
        file.write_all(&encoded)?;
        Ok(())
    }

    pub fn load_state() -> Option<ChainState> {
        if Path::new(STATE_FILE).exists() {
            if let Ok(mut file) = File::open(STATE_FILE) {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    if let Ok(state) = bincode::deserialize(&buffer) {
                        return Some(state);
                    }
                }
            }
        }
        None
    }
}
