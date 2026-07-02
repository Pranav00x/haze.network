pub mod core;
pub mod crypto;
pub mod wallet;

#[cfg(feature = "native")]
pub mod p2p;
#[cfg(feature = "native")]
pub mod api;
#[cfg(feature = "native")]
pub mod ffi;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "native")]
uniffi::setup_scaffolding!();
