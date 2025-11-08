//! Latch Engine Runtime
//!
//! Minimal binary that links engine crates and boots the game

use anyhow::Result;
use tracing_subscriber;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tracing::info!("Latch Engine v{}", latch_core::VERSION);
    tracing::info!("Initializing services...");
    latch_services::init_services();

    tracing::info!("Runtime initialized successfully");
    tracing::info!("Phase 0: Placeholder - will run game loop in PoC 1");

    Ok(())
}
