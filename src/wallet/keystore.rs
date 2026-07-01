use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use curve25519_dalek_ng::scalar::Scalar;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Serialize, Deserialize};
use sha2::{Sha512, Digest};

const WALLET_DIR: &str = "wallet_data";
const KEYSTORE_FILE: &str = "wallet_data/keystore.dat";

#[derive(Serialize, Deserialize)]
pub struct Keystore {
    seed: [u8; 32],
    next_index: u32,
}

impl Keystore {
    /// Loads the keystore from disk, or generates a fresh random seed if none exists.
    pub fn load_or_create() -> Self {
        if !Path::new(WALLET_DIR).exists() {
            fs::create_dir(WALLET_DIR).unwrap();
        }

        if Path::new(KEYSTORE_FILE).exists() {
            if let Ok(mut file) = File::open(KEYSTORE_FILE) {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    if let Ok(keystore) = bincode::deserialize::<Keystore>(&buffer) {
                        return keystore;
                    }
                }
            }
        }

        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let keystore = Keystore { seed, next_index: 0 };
        keystore.save();
        keystore
    }

    fn save(&self) {
        let encoded = bincode::serialize(self).unwrap();
        let mut file = File::create(KEYSTORE_FILE).unwrap();
        file.write_all(&encoded).unwrap();
    }

    /// Deterministically derives the blinding factor for a given output index.
    pub fn derive_blinding(&self, index: u32) -> Scalar {
        let mut hasher = Sha512::new();
        hasher.update(b"Haze Wallet Output Blinding");
        hasher.update(&self.seed);
        hasher.update(&index.to_le_bytes());
        let result = hasher.finalize();
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&result);
        Scalar::from_bytes_mod_order_wide(&bytes)
    }

    /// Allocates and persists a new output index, guaranteeing it is never reused.
    pub fn allocate_index(&mut self) -> u32 {
        let index = self.next_index;
        self.next_index += 1;
        self.save();
        index
    }
}
