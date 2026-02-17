//! 3-way structural merge
//!
//! Merges patches from two branches against a common ancestor.
//! Non-overlapping subtree edits merge cleanly; overlapping
//! edits on the same node produce conflicts.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec};

use crate::diff::DiffOp;
use crate::ast::NodeId;

/// Merge conflict
#[derive(Debug, Clone)]
pub struct Conflict {
    /// Node that has conflicting edits
    pub node_id: NodeId,
    /// Description of the conflict
    pub description: String,
    /// Operations from branch A
    pub ops_a: Vec<DiffOp>,
    /// Operations from branch B
    pub ops_b: Vec<DiffOp>,
}

/// Merge result
#[derive(Debug)]
pub struct MergeResult {
    /// Successfully merged operations
    pub merged_ops: Vec<DiffOp>,
    /// Conflicts that need manual resolution
    pub conflicts: Vec<Conflict>,
}

impl MergeResult {
    /// True if merge is clean (no conflicts)
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// Merge patches from two branches
///
/// `patch_a`: operations from branch A (relative to common ancestor)
/// `patch_b`: operations from branch B (relative to common ancestor)
///
/// Non-overlapping edits are combined. Overlapping edits on the
/// same node produce Conflict entries.
pub fn merge_patches(patch_a: &[DiffOp], patch_b: &[DiffOp]) -> MergeResult {
    let mut merged_ops = Vec::new();
    let mut conflicts = Vec::new();

    // Index: which nodes are affected by each patch
    let affected_a = affected_nodes(patch_a);
    let affected_b = affected_nodes(patch_b);

    // Non-conflicting ops from A
    for op in patch_a {
        let node = op_target_node(op);
        if !affected_b.contains(&node) {
            merged_ops.push(op.clone());
        }
    }

    // Non-conflicting ops from B
    for op in patch_b {
        let node = op_target_node(op);
        if !affected_a.contains(&node) {
            merged_ops.push(op.clone());
        }
    }

    // Conflicting nodes
    for &node_id in &affected_a {
        if affected_b.contains(&node_id) {
            let ops_a: Vec<_> = patch_a
                .iter()
                .filter(|o| op_target_node(o) == node_id)
                .cloned()
                .collect();
            let ops_b: Vec<_> = patch_b
                .iter()
                .filter(|o| op_target_node(o) == node_id)
                .cloned()
                .collect();

            // Check if both patches do the same thing (auto-resolve)
            if ops_a == ops_b {
                merged_ops.extend(ops_a);
            } else {
                conflicts.push(Conflict {
                    node_id,
                    description: String::from("conflicting edits on same node"),
                    ops_a,
                    ops_b,
                });
            }
        }
    }

    MergeResult {
        merged_ops,
        conflicts,
    }
}

/// Get the target node of an operation
fn op_target_node(op: &DiffOp) -> NodeId {
    match op {
        DiffOp::Insert { parent_id, .. } => *parent_id,
        DiffOp::Delete { node_id } => *node_id,
        DiffOp::Update { node_id, .. } => *node_id,
        DiffOp::Relabel { node_id, .. } => *node_id,
        DiffOp::Move { node_id, .. } => *node_id,
    }
}

/// Collect all node IDs affected by a patch
fn affected_nodes(ops: &[DiffOp]) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    for op in ops {
        let id = op_target_node(op);
        if !nodes.contains(&id) {
            nodes.push(id);
        }
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::NodeValue;
    use crate::diff::DiffOp;

    #[test]
    fn test_clean_merge_non_overlapping() {
        let patch_a = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let patch_b = vec![DiffOp::Update {
            node_id: 2,
            old_value: NodeValue::Float(3.0),
            new_value: NodeValue::Float(4.0),
        }];

        let result = merge_patches(&patch_a, &patch_b);
        assert!(result.is_clean());
        assert_eq!(result.merged_ops.len(), 2);
    }

    #[test]
    fn test_conflict_same_node_different_values() {
        let patch_a = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let patch_b = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(3.0),
        }];

        let result = merge_patches(&patch_a, &patch_b);
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].node_id, 1);
    }

    #[test]
    fn test_auto_resolve_identical_changes() {
        let patch_a = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let patch_b = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];

        let result = merge_patches(&patch_a, &patch_b);
        assert!(result.is_clean());
        assert_eq!(result.merged_ops.len(), 1);
    }

    #[test]
    fn test_empty_merge() {
        let result = merge_patches(&[], &[]);
        assert!(result.is_clean());
        assert!(result.merged_ops.is_empty());
    }
}
