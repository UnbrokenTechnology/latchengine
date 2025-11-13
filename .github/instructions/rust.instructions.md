---
applyTo: "**/*.rs"
---

# Rust Instructions

## Unsafe Rust

We aren't afraid of Unsafe Rust in this repository, however, we do have some guidelines to ensure safety:

 * Unsafe blocks should be minimal and well-documented
 * Unsafe code must verify all assumptions at runtime

## Error Handling

 * Use `Result<T, E>` for ALL error, recoverable or not. This lets us log errors and show them in the UI.
 * Define custom error types for each module, implementing `std::error::Error` and `std::fmt::Display`.

## File Structure

 * Each public struct should be in its own file, named after the struct (e.g. `component.rs` for `Component`).
 * If a private struct is a Plain Old Data (POD) type, it can be defined in the same file as its parent struct. If it has logic, it should be in its own file.
 * Each module should have a `mod.rs` that re-exports its public API.

## Tests

 * We do not use `#[cfg(test)]` or `#[test]` functions. We do not care about unit tests.
 * Instead, we have a separate `tests/` directory with integration tests.