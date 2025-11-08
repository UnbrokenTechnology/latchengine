//! Server discovery and membership
//!
//! SWIM/Serf-style gossip protocol for self-organization

use crate::NodeId;
use std::collections::HashMap;

/// Node state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    Alive,
    Suspected,
    Dead,
}

/// Membership table
pub struct MembershipTable {
    nodes: HashMap<NodeId, NodeState>,
}

impl MembershipTable {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: NodeId) {
        self.nodes.insert(node, NodeState::Alive);
    }

    pub fn mark_suspected(&mut self, node: NodeId) {
        if let Some(state) = self.nodes.get_mut(&node) {
            *state = NodeState::Suspected;
        }
    }

    pub fn mark_dead(&mut self, node: NodeId) {
        if let Some(state) = self.nodes.get_mut(&node) {
            *state = NodeState::Dead;
        }
    }

    pub fn alive_nodes(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|(_, &state)| state == NodeState::Alive)
            .map(|(&id, _)| id)
            .collect()
    }
}

impl Default for MembershipTable {
    fn default() -> Self {
        Self::new()
    }
}
