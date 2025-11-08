//! Latch Scripting System
//!
//! TypeScript/JavaScript execution via QuickJS (dev) and WASM (ship)

pub mod ffi;
pub mod runtime;

pub use rquickjs;
