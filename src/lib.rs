#[cfg(feature = "native")]
pub mod ffi;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "native")]
uniffi::setup_scaffolding!();
