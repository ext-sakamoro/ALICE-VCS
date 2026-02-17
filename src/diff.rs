//! AST diff engine
//!
//! Computes minimal edit operations between two AST trees.
//! Produces operation-based patches: Insert, Delete, Update, Move.
//! Each operation is 4-12 bytes — vs 50 KB for binary diffs.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec};

use crate::ast::{AstNodeKind, AstTree, NodeId, NodeValue};

/// Diff operation on AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum DiffOp {
    /// Insert a new node
    Insert {
        parent_id: NodeId,
        index: usize,
        kind: AstNodeKind,
        label: String,
        value: NodeValue,
    },
    /// Delete a node (and its subtree)
    Delete {
        node_id: NodeId,
    },
    /// Update a node's value
    Update {
        node_id: NodeId,
        old_value: NodeValue,
        new_value: NodeValue,
    },
    /// Update a node's label
    Relabel {
        node_id: NodeId,
        old_label: String,
        new_label: String,
    },
    /// Move a node to a new parent
    Move {
        node_id: NodeId,
        new_parent_id: NodeId,
        new_index: usize,
    },
}

impl DiffOp {
    /// Estimated serialized size in bytes
    pub fn serialized_size(&self) -> usize {
        match self {
            DiffOp::Insert { label, value, .. } => 8 + label.len() + value.serialized_size(),
            DiffOp::Delete { .. } => 5,
            DiffOp::Update { new_value, .. } => 5 + new_value.serialized_size(),
            DiffOp::Relabel { new_label, .. } => 5 + new_label.len(),
            DiffOp::Move { .. } => 12,
        }
    }
}

/// Compute diff operations to transform `old` tree into `new` tree
///
/// Uses a simplified tree-edit approach:
/// 1. Match nodes by (kind, label) between old and new
/// 2. Unmatched in old → Delete
/// 3. Unmatched in new → Insert
/// 4. Matched but changed value → Update
pub fn diff_trees(old: &AstTree, new: &AstTree) -> Vec<DiffOp> {
    let mut ops = Vec::new();
    diff_subtree(old, new, old.root_id(), new.root_id(), &mut ops);
    ops
}

fn diff_subtree(
    old: &AstTree,
    new: &AstTree,
    old_id: NodeId,
    new_id: NodeId,
    ops: &mut Vec<DiffOp>,
) {
    let old_node = match old.get_node(old_id) {
        Some(n) => n,
        None => return,
    };
    let new_node = match new.get_node(new_id) {
        Some(n) => n,
        None => return,
    };

    // Check for label change
    if old_node.label != new_node.label {
        ops.push(DiffOp::Relabel {
            node_id: old_id,
            old_label: old_node.label.clone(),
            new_label: new_node.label.clone(),
        });
    }

    // Check for value change
    if old_node.value != new_node.value {
        ops.push(DiffOp::Update {
            node_id: old_id,
            old_value: old_node.value.clone(),
            new_value: new_node.value.clone(),
        });
    }

    // Diff children
    let old_children = &old_node.children.clone();
    let new_children = &new_node.children.clone();

    // Match children by (kind, label)
    let mut matched_new: Vec<bool> = vec![false; new_children.len()];
    let mut matched_old: Vec<bool> = vec![false; old_children.len()];

    // First pass: exact matches by label
    for (oi, &old_child_id) in old_children.iter().enumerate() {
        if let Some(old_child) = old.get_node(old_child_id) {
            for (ni, &new_child_id) in new_children.iter().enumerate() {
                if matched_new[ni] {
                    continue;
                }
                if let Some(new_child) = new.get_node(new_child_id) {
                    if old_child.kind == new_child.kind && old_child.label == new_child.label {
                        matched_old[oi] = true;
                        matched_new[ni] = true;
                        // Recurse
                        diff_subtree(old, new, old_child_id, new_child_id, ops);
                        break;
                    }
                }
            }
        }
    }

    // Deleted: unmatched in old
    for (oi, &old_child_id) in old_children.iter().enumerate() {
        if !matched_old[oi] {
            ops.push(DiffOp::Delete {
                node_id: old_child_id,
            });
        }
    }

    // Inserted: unmatched in new
    for (ni, &new_child_id) in new_children.iter().enumerate() {
        if !matched_new[ni] {
            if let Some(new_child) = new.get_node(new_child_id) {
                ops.push(DiffOp::Insert {
                    parent_id: old_id,
                    index: ni,
                    kind: new_child.kind,
                    label: new_child.label.clone(),
                    value: new_child.value.clone(),
                });
            }
        }
    }
}

/// Apply diff operations to an AST tree (returns new tree)
pub fn apply_patch(tree: &mut AstTree, ops: &[DiffOp]) {
    for op in ops {
        match op {
            DiffOp::Insert {
                parent_id,
                kind,
                label,
                value,
                ..
            } => {
                tree.add_node_with_value(*kind, label, value.clone(), *parent_id);
            }
            DiffOp::Delete { node_id } => {
                tree.remove_subtree(*node_id);
            }
            DiffOp::Update {
                node_id, new_value, ..
            } => {
                if let Some(node) = tree.get_node_mut(*node_id) {
                    node.value = new_value.clone();
                }
            }
            DiffOp::Relabel {
                node_id, new_label, ..
            } => {
                if let Some(node) = tree.get_node_mut(*node_id) {
                    node.label = new_label.clone();
                }
            }
            DiffOp::Move {
                node_id,
                new_parent_id,
                ..
            } => {
                // Remove from old parent
                if let Some(old_parent_id) = tree.parent_of(*node_id) {
                    if let Some(parent) = tree.get_node_mut(old_parent_id) {
                        parent.children.retain(|&c| c != *node_id);
                    }
                }
                // Add to new parent
                if let Some(parent) = tree.get_node_mut(*new_parent_id) {
                    parent.children.push(*node_id);
                }
            }
        }
    }
}

/// Total patch size in bytes
pub fn patch_size_bytes(ops: &[DiffOp]) -> usize {
    ops.iter().map(|op| op.serialized_size()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNodeKind, AstTree, NodeValue};

    #[test]
    fn test_no_diff_identical_trees() {
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);
        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);

        let ops = diff_trees(&t1, &t2);
        assert!(ops.is_empty(), "identical trees should have no diff");
    }

    #[test]
    fn test_diff_value_change() {
        let mut t1 = AstTree::new();
        let s = t1.add_node(AstNodeKind::Primitive, "sphere", 0);
        t1.add_node_with_value(AstNodeKind::Parameter, "radius", NodeValue::Float(1.0), s);

        let mut t2 = AstTree::new();
        let s2 = t2.add_node(AstNodeKind::Primitive, "sphere", 0);
        t2.add_node_with_value(AstNodeKind::Parameter, "radius", NodeValue::Float(1.5), s2);

        let ops = diff_trees(&t1, &t2);
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], DiffOp::Update { .. }));
    }

    #[test]
    fn test_diff_insert() {
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);

        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);
        t2.add_node(AstNodeKind::Primitive, "box", 0);

        let ops = diff_trees(&t1, &t2);
        let inserts: Vec<_> = ops.iter().filter(|o| matches!(o, DiffOp::Insert { .. })).collect();
        assert_eq!(inserts.len(), 1);
    }

    #[test]
    fn test_diff_delete() {
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);
        t1.add_node(AstNodeKind::Primitive, "box", 0);

        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);

        let ops = diff_trees(&t1, &t2);
        let deletes: Vec<_> = ops.iter().filter(|o| matches!(o, DiffOp::Delete { .. })).collect();
        assert_eq!(deletes.len(), 1);
    }

    #[test]
    fn test_patch_size_small() {
        let mut t1 = AstTree::new();
        let s = t1.add_node(AstNodeKind::Primitive, "sphere", 0);
        t1.add_node_with_value(AstNodeKind::Parameter, "radius", NodeValue::Float(1.0), s);

        let mut t2 = AstTree::new();
        let s2 = t2.add_node(AstNodeKind::Primitive, "sphere", 0);
        t2.add_node_with_value(AstNodeKind::Parameter, "radius", NodeValue::Float(1.5), s2);

        let ops = diff_trees(&t1, &t2);
        let size = patch_size_bytes(&ops);
        assert!(size < 20, "value change patch should be < 20 bytes, got {size}");
    }

    #[test]
    fn test_apply_patch() {
        let mut tree = AstTree::new();
        let s = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        tree.add_node_with_value(AstNodeKind::Parameter, "radius", NodeValue::Float(1.0), s);

        let ops = vec![DiffOp::Update {
            node_id: 2,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        apply_patch(&mut tree, &ops);
        let node = tree.get_node(2).unwrap();
        assert_eq!(node.value, NodeValue::Float(2.0));
    }
}
