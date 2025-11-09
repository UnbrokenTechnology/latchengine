//! Latch Scripting System
//!
//! TypeScript/JavaScript execution via QuickJS (dev) and WASM (ship)
//!
//! ## Architecture
//!
//! - **Dev mode:** QuickJS for instant hot reload
//! - **Ship mode:** WASM via AssemblyScript for performance
//! - **FFI:** Zero-copy via SharedArrayBuffer (WASM) or direct array passing (QuickJS)
//!
//! See examples/poc4_typescript_logic.rs and examples/poc4_wasm_zero_copy.rs
//! for working implementations.

pub mod runtime;

pub use rquickjs;
