//! Latch Network Layer
//!
//! Distributed authority, cell-based world partitioning, and self-organizing servers

pub mod authority;
pub mod cell;
pub mod discovery;
pub mod replication;

/// Network protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Cell ID (spatial partition identifier)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct CellId(pub u64);

/// Server node ID
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);
