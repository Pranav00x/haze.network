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

        let keystore = Self::generate();
        keystore.save_to_file();
        keystore
    }

    /// Persists the keystore to the CLI's fixed on-disk location. Only meaningful
    /// for file-backed usage (the CLI wallet) - callers managing their own
    /// persistence (e.g. mobile FFI) should use to_bytes()/from_bytes() instead and
    /// never need this.
    pub fn save_to_file(&self) {
        if !Path::new(WALLET_DIR).exists() {
            fs::create_dir(WALLET_DIR).unwrap();
        }
        let encoded = self.to_bytes();
        let mut file = File::create(KEYSTORE_FILE).unwrap();
        file.write_all(&encoded).unwrap();
    }

    /// Serializes the keystore to bytes, for callers (e.g. mobile FFI) that manage
    /// their own persistence instead of using load_or_create()'s file-based storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    /// Reconstructs a keystore from bytes previously produced by to_bytes().
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }

    /// Generates a fresh keystore with a random seed, without touching disk.
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        Keystore { seed, next_index: 0 }
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

    /// Deterministically derives this wallet's stable naming-registry identity
    /// key - separate from any output's blinding (a different domain-separated
    /// derivation, not tied to an index), since a NameRecord's owner/resolves-to
    /// pubkey is a persistent identity, not a spendable output.
    pub fn identity_key(&self) -> Scalar {
        let mut hasher = Sha512::new();
        hasher.update(b"Haze Wallet Naming Identity");
        hasher.update(&self.seed);
        let result = hasher.finalize();
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&result);
        Scalar::from_bytes_mod_order_wide(&bytes)
    }

    /// Allocates a new output index, guaranteeing it is never reused within this
    /// Keystore value. Purely in-memory - does not touch disk. File-backed callers
    /// (the CLI wallet) must call save_to_file() themselves right after allocating,
    /// before doing anything (e.g. broadcasting) that shouldn't be repeated on a
    /// crash; callers managing their own persistence (mobile FFI) just take the
    /// updated bytes via to_bytes() and store them however they like.
    pub fn allocate_index(&mut self) -> u32 {
        let index = self.next_index;
        self.next_index += 1;
        index
    }
}
