//! Latch Engine Editor
//!
//! Editor application with GUI, scene view, and development tools

use anyhow::Result;
use tracing_subscriber;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tracing::info!("Latch Editor v{}", latch_core::VERSION);
    tracing::info!("Editor initialized successfully");
    tracing::info!("Phase 0: Placeholder - will add editor UI in Phase 3");

    Ok(())
}
