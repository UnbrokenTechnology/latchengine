//! Authority management
//!
//! Determines which server owns which cells

use crate::{CellId, NodeId};

/// Authority assignment
pub struct AuthorityMap {
    // Placeholder: will use consistent hashing
}

impl AuthorityMap {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_authority(&self, _cell: CellId) -> Option<NodeId> {
        // Phase 0: stub
        None
    }

    pub fn assign_authority(&mut self, _cell: CellId, _node: NodeId) {
        // Phase 0: stub
    }
}

impl Default for AuthorityMap {
    fn default() -> Self {
        Self::new()
    }
}
