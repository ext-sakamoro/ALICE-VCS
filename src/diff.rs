//! AST diff engine
//!
//! Computes minimal edit operations between two AST trees.
//! Produces operation-based patches: Insert, Delete, Update, Move.
//! Each operation is 4-12 bytes — vs 50 KB for binary diffs.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec};
#[cfg(feature = "std")]
use std::collections::HashMap;

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
    Delete { node_id: NodeId },
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
/// 1. Match nodes by (kind, label) between old and new using O(1) HashMap lookup
/// 2. Unmatched in old -> Delete
/// 3. Unmatched in new -> Insert
/// 4. Matched but changed value -> Update
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
    let old_children = old_node.children.clone();
    let new_children = new_node.children.clone();

    // Build a HashMap from (kind, label) -> list of new child indices for O(1) matching.
    // The previous implementation used nested loops: O(m*n) per level.
    // This HashMap approach reduces child matching to O(m+n) per level.
    let mut new_key_to_indices: HashMap<(AstNodeKind, String), Vec<usize>> = HashMap::new();
    for (ni, &new_child_id) in new_children.iter().enumerate() {
        if let Some(new_child) = new.get_node(new_child_id) {
            new_key_to_indices
                .entry((new_child.kind, new_child.label.clone()))
                .or_default()
                .push(ni);
        }
    }

    let mut matched_new: Vec<bool> = vec![false; new_children.len()];
    let mut matched_old: Vec<bool> = vec![false; old_children.len()];

    // First pass: match old children to new children via HashMap lookup — O(n)
    for (oi, &old_child_id) in old_children.iter().enumerate() {
        if let Some(old_child) = old.get_node(old_child_id) {
            let key = (old_child.kind, old_child.label.clone());
            if let Some(candidates) = new_key_to_indices.get_mut(&key) {
                // Find the first unmatched candidate for this key
                if let Some(pos) = candidates.iter().position(|&ni| !matched_new[ni]) {
                    let ni = candidates[pos];
                    matched_old[oi] = true;
                    matched_new[ni] = true;
                    // Recurse into matched pair
                    diff_subtree(old, new, old_child_id, new_children[ni], ops);
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

/// Apply diff operations to an AST tree
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
    #[cfg(not(feature = "std"))]
    use alloc::format;

    // ── Original tests ─────────────────────────────────────────────────

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
        let inserts: Vec<_> = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Insert { .. }))
            .collect();
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
        let deletes: Vec<_> = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Delete { .. }))
            .collect();
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
        assert!(
            size < 20,
            "value change patch should be < 20 bytes, got {size}"
        );
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

    // ── New tests ──────────────────────────────────────────────────────

    #[test]
    fn test_empty_trees_no_diff() {
        let t1 = AstTree::new();
        let t2 = AstTree::new();
        assert!(diff_trees(&t1, &t2).is_empty());
    }

    #[test]
    fn test_diff_label_swap_produces_delete_and_insert() {
        // sphere -> cylinder: no match by (kind,label) -> delete + insert
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);

        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "cylinder", 0);

        let ops = diff_trees(&t1, &t2);
        let deletes = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Delete { .. }))
            .count();
        let inserts = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Insert { .. }))
            .count();
        assert_eq!(deletes, 1);
        assert_eq!(inserts, 1);
    }

    #[test]
    fn test_diff_multiple_inserts() {
        let t1 = AstTree::new(); // root only

        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "a", 0);
        t2.add_node(AstNodeKind::Primitive, "b", 0);
        t2.add_node(AstNodeKind::Primitive, "c", 0);

        let ops = diff_trees(&t1, &t2);
        let inserts = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Insert { .. }))
            .count();
        assert_eq!(inserts, 3);
    }

    #[test]
    fn test_diff_multiple_deletes() {
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "a", 0);
        t1.add_node(AstNodeKind::Primitive, "b", 0);
        t1.add_node(AstNodeKind::Primitive, "c", 0);

        let t2 = AstTree::new(); // root only

        let ops = diff_trees(&t1, &t2);
        let deletes = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Delete { .. }))
            .count();
        assert_eq!(deletes, 3);
    }

    #[test]
    fn test_diff_update_int_value() {
        let mut t1 = AstTree::new();
        let g = t1.add_node(AstNodeKind::Group, "g", 0);
        t1.add_node_with_value(AstNodeKind::Parameter, "count", NodeValue::Int(10), g);

        let mut t2 = AstTree::new();
        let g2 = t2.add_node(AstNodeKind::Group, "g", 0);
        t2.add_node_with_value(AstNodeKind::Parameter, "count", NodeValue::Int(99), g2);

        let ops = diff_trees(&t1, &t2);
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            DiffOp::Update { new_value, .. } => assert_eq!(*new_value, NodeValue::Int(99)),
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn test_diff_update_text_value() {
        let mut t1 = AstTree::new();
        let g = t1.add_node(AstNodeKind::Material, "mat", 0);
        t1.add_node_with_value(
            AstNodeKind::Parameter,
            "name",
            NodeValue::Text(String::from("old")),
            g,
        );

        let mut t2 = AstTree::new();
        let g2 = t2.add_node(AstNodeKind::Material, "mat", 0);
        t2.add_node_with_value(
            AstNodeKind::Parameter,
            "name",
            NodeValue::Text(String::from("new")),
            g2,
        );

        let ops = diff_trees(&t1, &t2);
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], DiffOp::Update { .. }));
    }

    #[test]
    fn test_diff_deep_nested_no_change() {
        let mut t1 = AstTree::new();
        let g = t1.add_node(AstNodeKind::Group, "g", 0);
        let p = t1.add_node(AstNodeKind::Primitive, "sphere", g);
        t1.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(1.0), p);

        let mut t2 = AstTree::new();
        let g2 = t2.add_node(AstNodeKind::Group, "g", 0);
        let p2 = t2.add_node(AstNodeKind::Primitive, "sphere", g2);
        t2.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(1.0), p2);

        assert!(diff_trees(&t1, &t2).is_empty());
    }

    #[test]
    fn test_diff_deep_nested_value_change() {
        let mut t1 = AstTree::new();
        let g = t1.add_node(AstNodeKind::Group, "g", 0);
        let p = t1.add_node(AstNodeKind::Primitive, "sphere", g);
        t1.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(1.0), p);

        let mut t2 = AstTree::new();
        let g2 = t2.add_node(AstNodeKind::Group, "g", 0);
        let p2 = t2.add_node(AstNodeKind::Primitive, "sphere", g2);
        t2.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(5.0), p2);

        let ops = diff_trees(&t1, &t2);
        let updates = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Update { .. }))
            .count();
        assert_eq!(updates, 1);
    }

    #[test]
    fn test_diff_same_label_different_kind_is_unmatched() {
        // Same label but different kind must NOT match
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "x", 0);

        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Group, "x", 0);

        let ops = diff_trees(&t1, &t2);
        let deletes = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Delete { .. }))
            .count();
        let inserts = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Insert { .. }))
            .count();
        assert_eq!(deletes, 1);
        assert_eq!(inserts, 1);
    }

    #[test]
    fn test_diff_duplicate_labels_match_once_each() {
        // Two identical children in old; two identical in new — zero ops
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);

        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);

        assert!(diff_trees(&t1, &t2).is_empty());
    }

    #[test]
    fn test_diff_one_duplicate_added() {
        // Old has one "sphere"; new has two — one Insert expected
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);

        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);

        let ops = diff_trees(&t1, &t2);
        let inserts = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Insert { .. }))
            .count();
        assert_eq!(inserts, 1);
    }

    #[test]
    fn test_apply_patch_delete() {
        let mut tree = AstTree::new();
        let id = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        assert_eq!(tree.node_count(), 2);

        apply_patch(&mut tree, &[DiffOp::Delete { node_id: id }]);
        assert_eq!(tree.node_count(), 1);
    }

    #[test]
    fn test_apply_patch_insert() {
        let mut tree = AstTree::new();
        assert_eq!(tree.node_count(), 1);

        apply_patch(
            &mut tree,
            &[DiffOp::Insert {
                parent_id: 0,
                index: 0,
                kind: AstNodeKind::Primitive,
                label: String::from("box"),
                value: NodeValue::None,
            }],
        );
        assert_eq!(tree.node_count(), 2);
    }

    #[test]
    fn test_apply_patch_relabel() {
        let mut tree = AstTree::new();
        let id = tree.add_node(AstNodeKind::Primitive, "sphere", 0);

        apply_patch(
            &mut tree,
            &[DiffOp::Relabel {
                node_id: id,
                old_label: String::from("sphere"),
                new_label: String::from("cylinder"),
            }],
        );
        assert_eq!(tree.get_node(id).unwrap().label, "cylinder");
    }

    #[test]
    fn test_apply_patch_move() {
        let mut tree = AstTree::new();
        let g1 = tree.add_node(AstNodeKind::Group, "g1", 0);
        let g2 = tree.add_node(AstNodeKind::Group, "g2", 0);
        let child = tree.add_node(AstNodeKind::Primitive, "s", g1);

        apply_patch(
            &mut tree,
            &[DiffOp::Move {
                node_id: child,
                new_parent_id: g2,
                new_index: 0,
            }],
        );
        assert!(!tree.get_node(g1).unwrap().children.contains(&child));
        assert!(tree.get_node(g2).unwrap().children.contains(&child));
    }

    #[test]
    fn test_apply_patch_multiple_ops() {
        let mut tree = AstTree::new();
        let s = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        tree.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(1.0), s);

        let ops = vec![
            DiffOp::Insert {
                parent_id: 0,
                index: 1,
                kind: AstNodeKind::Primitive,
                label: String::from("box"),
                value: NodeValue::None,
            },
            DiffOp::Update {
                node_id: 2,
                old_value: NodeValue::Float(1.0),
                new_value: NodeValue::Float(3.0),
            },
        ];
        apply_patch(&mut tree, &ops);
        assert_eq!(tree.node_count(), 4); // root + sphere + r + box
        assert_eq!(tree.get_node(2).unwrap().value, NodeValue::Float(3.0));
    }

    #[test]
    fn test_patch_size_bytes_empty() {
        assert_eq!(patch_size_bytes(&[]), 0);
    }

    #[test]
    fn test_patch_size_bytes_delete_is_5() {
        let ops = vec![DiffOp::Delete { node_id: 1 }];
        assert_eq!(patch_size_bytes(&ops), 5);
    }

    #[test]
    fn test_patch_size_bytes_move_is_12() {
        let ops = vec![DiffOp::Move {
            node_id: 1,
            new_parent_id: 2,
            new_index: 0,
        }];
        assert_eq!(patch_size_bytes(&ops), 12);
    }

    #[test]
    fn test_diff_csg_tree_no_change() {
        let build = |tree: &mut AstTree| {
            let u = tree.add_node(AstNodeKind::CsgOp, "union", 0);
            tree.add_node_with_value(AstNodeKind::Primitive, "sphere", NodeValue::Float(1.0), u);
            tree.add_node_with_value(AstNodeKind::Primitive, "box", NodeValue::Float(0.5), u);
        };
        let mut t1 = AstTree::new();
        build(&mut t1);
        let mut t2 = AstTree::new();
        build(&mut t2);
        assert!(diff_trees(&t1, &t2).is_empty());
    }

    #[test]
    fn test_diff_csg_tree_one_param_changed() {
        let mut t1 = AstTree::new();
        let u = t1.add_node(AstNodeKind::CsgOp, "union", 0);
        let s = t1.add_node(AstNodeKind::Primitive, "sphere", u);
        t1.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(1.0), s);

        let mut t2 = AstTree::new();
        let u2 = t2.add_node(AstNodeKind::CsgOp, "union", 0);
        let s2 = t2.add_node(AstNodeKind::Primitive, "sphere", u2);
        t2.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(2.0), s2);

        let ops = diff_trees(&t1, &t2);
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], DiffOp::Update { .. }));
    }

    #[test]
    fn test_serialized_size_insert() {
        let op = DiffOp::Insert {
            parent_id: 0,
            index: 0,
            kind: AstNodeKind::Primitive,
            label: String::from("ab"), // 2 bytes
            value: NodeValue::None,    // 1 byte
        };
        // 8 + label.len() + value.serialized_size() = 8 + 2 + 1 = 11
        assert_eq!(op.serialized_size(), 11);
    }

    #[test]
    fn test_serialized_size_relabel() {
        let op = DiffOp::Relabel {
            node_id: 1,
            old_label: String::from("sphere"),
            new_label: String::from("cylinder"), // 8 bytes
        };
        // 5 + new_label.len() = 5 + 8 = 13
        assert_eq!(op.serialized_size(), 13);
    }

    #[test]
    fn test_diff_and_apply_roundtrip() {
        let mut t1 = AstTree::new();
        let s = t1.add_node(AstNodeKind::Primitive, "sphere", 0);
        t1.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(1.0), s);

        let mut t2 = AstTree::new();
        let s2 = t2.add_node(AstNodeKind::Primitive, "sphere", 0);
        t2.add_node_with_value(AstNodeKind::Parameter, "r", NodeValue::Float(5.0), s2);

        let ops = diff_trees(&t1, &t2);
        apply_patch(&mut t1, &ops);

        // The radius parameter node id is 2 in t1
        assert_eq!(t1.get_node(2).unwrap().value, NodeValue::Float(5.0));
    }

    #[test]
    fn test_diff_large_flat_tree_no_change() {
        // 50 children: verifies HashMap path is correct at scale
        let build = |tree: &mut AstTree| {
            for i in 0u32..50 {
                tree.add_node(AstNodeKind::Primitive, &format!("node_{i}"), 0);
            }
        };
        let mut t1 = AstTree::new();
        build(&mut t1);
        let mut t2 = AstTree::new();
        build(&mut t2);
        assert!(diff_trees(&t1, &t2).is_empty());
    }

    #[test]
    fn test_diff_large_flat_tree_one_changed() {
        // 50 children; only node_25's child value changes
        let mut t1 = AstTree::new();
        let mut t2 = AstTree::new();
        for i in 0u32..50 {
            let label = format!("node_{i}");
            let n1 = t1.add_node(AstNodeKind::Primitive, &label, 0);
            let n2 = t2.add_node(AstNodeKind::Primitive, &label, 0);
            let val1 = if i == 25 {
                NodeValue::Float(1.0)
            } else {
                NodeValue::None
            };
            let val2 = if i == 25 {
                NodeValue::Float(9.9)
            } else {
                NodeValue::None
            };
            t1.add_node_with_value(AstNodeKind::Parameter, "v", val1, n1);
            t2.add_node_with_value(AstNodeKind::Parameter, "v", val2, n2);
        }

        let ops = diff_trees(&t1, &t2);
        let updates = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Update { .. }))
            .count();
        assert_eq!(updates, 1, "only one node value changed");
    }
}
