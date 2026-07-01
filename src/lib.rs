pub mod core;
pub mod crypto;
pub mod p2p;
pub mod api;
pub mod wallet;
pub mod ffi;

uniffi::setup_scaffolding!();
