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

    /// Generates a fresh keystore backed by a real BIP39 mnemonic - same
    /// standard as most wallets (MetaMask, etc), so the phrase can be written
    /// down and used to recover the wallet later via from_mnemonic(). Returns
    /// the phrase alongside the keystore since it's only ever available at
    /// generation time - the keystore itself never stores it.
    pub fn generate_with_mnemonic() -> (Self, String) {
        let mut entropy = [0u8; 16]; // 128 bits -> 12-word mnemonic
        OsRng.fill_bytes(&mut entropy);
        let mnemonic = bip39::Mnemonic::from_entropy(&entropy).expect("valid entropy length");
        let phrase = mnemonic.to_string();
        let keystore = Self::from_mnemonic(&phrase).expect("just-generated mnemonic must parse");
        (keystore, phrase)
    }

    /// Constructs a keystore from an already-derived 32-byte seed, rather
    /// than generating a fresh random one or parsing a BIP39 phrase - used
    /// by api::faucet::FaucetState to derive a stable, restart-proof
    /// identity from its own treasury secret, the same way a user's phrase
    /// deterministically derives theirs.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Keystore { seed, next_index: 0 }
    }

    /// Reconstructs a keystore deterministically from a previously-generated
    /// BIP39 phrase - the same phrase always yields the same seed, and thus
    /// the same keys/outputs/identity.
    pub fn from_mnemonic(phrase: &str) -> Option<Self> {
        let mnemonic = bip39::Mnemonic::parse_normalized(phrase).ok()?;
        let bip39_seed = mnemonic.to_seed("");
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bip39_seed[0..32]);
        Some(Keystore { seed, next_index: 0 })
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

    /// Deterministically derives this wallet's symmetric note-encryption key -
    /// separate from derive_blinding/identity_key (a different domain-separated
    /// derivation), used to seal/open each output's recoverable note (see
    /// wallet::note). Unlike a per-output blinding factor, this one key covers
    /// every output the wallet ever creates, since restoring from a phrase has
    /// no local index bookkeeping to know which indices to even check.
    pub fn note_key(&self) -> [u8; 32] {
        let mut hasher = Sha512::new();
        hasher.update(b"Haze Wallet Note Key");
        hasher.update(&self.seed);
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result[0..32]);
        key
    }

    /// Bumps next_index up to at least `min`, without ever decreasing it -
    /// used after recovering outputs by scanning the chain (see wallet::note),
    /// so newly allocated indices can't collide with ones a restored wallet
    /// already used before it lost its local next_index bookkeeping.
    pub fn ensure_next_index_at_least(&mut self, min: u32) {
        if self.next_index < min {
            self.next_index = min;
        }
    }
}
