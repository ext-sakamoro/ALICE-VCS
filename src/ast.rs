//! Abstract Syntax Tree representation
//!
//! Generic tree structure for procedural data (SDF CSG, scene graphs,
//! panel layouts). Each node has a kind, optional value, and children.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec};
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;
#[cfg(feature = "std")]
use std::collections::HashMap;

/// Unique node identifier
pub type NodeId = u32;

/// AST node kind — what type of procedural entity this represents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum AstNodeKind {
    /// Root of the tree
    Root = 0,
    /// CSG operation (union, subtract, intersect)
    CsgOp = 1,
    /// Geometric primitive (sphere, box, cylinder)
    Primitive = 2,
    /// Transform (translate, rotate, scale)
    Transform = 3,
    /// Numeric parameter (radius, width, angle)
    Parameter = 4,
    /// Scene/group node
    Group = 5,
    /// Material / shader definition
    Material = 6,
    /// Animation keyframe
    Keyframe = 7,
    /// Custom / extension node
    Custom = 255,
}

impl AstNodeKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Root,
            1 => Self::CsgOp,
            2 => Self::Primitive,
            3 => Self::Transform,
            4 => Self::Parameter,
            5 => Self::Group,
            6 => Self::Material,
            7 => Self::Keyframe,
            _ => Self::Custom,
        }
    }
}

/// Value attached to a node
#[derive(Debug, Clone, PartialEq)]
pub enum NodeValue {
    /// No value
    None,
    /// Integer value
    Int(i64),
    /// Float value
    Float(f64),
    /// String value
    Text(String),
    /// Named identifier (e.g. "sphere", "union")
    Ident(String),
    /// Raw bytes
    Bytes(Vec<u8>),
}

impl NodeValue {
    /// Size in bytes when serialized
    pub fn serialized_size(&self) -> usize {
        match self {
            NodeValue::None => 1,
            NodeValue::Int(_) => 9,
            NodeValue::Float(_) => 9,
            NodeValue::Text(s) => 3 + s.len(),
            NodeValue::Ident(s) => 3 + s.len(),
            NodeValue::Bytes(b) => 3 + b.len(),
        }
    }
}

/// AST node
#[derive(Debug, Clone)]
pub struct AstNode {
    /// Unique identifier within the tree
    pub id: NodeId,
    /// Node kind
    pub kind: AstNodeKind,
    /// Node label (e.g. "sphere", "translate")
    pub label: String,
    /// Attached value
    pub value: NodeValue,
    /// Child node IDs
    pub children: Vec<NodeId>,
}

impl AstNode {
    pub fn new(id: NodeId, kind: AstNodeKind, label: &str) -> Self {
        Self {
            id,
            kind,
            label: String::from(label),
            value: NodeValue::None,
            children: Vec::new(),
        }
    }

    pub fn with_value(mut self, value: NodeValue) -> Self {
        self.value = value;
        self
    }

    pub fn with_children(mut self, children: Vec<NodeId>) -> Self {
        self.children = children;
        self
    }
}

/// AST tree — flat storage of nodes with O(1) ID lookup via HashMap index
#[derive(Debug, Clone)]
pub struct AstTree {
    nodes: Vec<AstNode>,
    /// Maps NodeId → index in `nodes` Vec for O(1) lookup
    index: HashMap<NodeId, usize>,
    /// Maps child NodeId → parent NodeId for O(1) parent lookup
    parent_index: HashMap<NodeId, NodeId>,
    root_id: NodeId,
    next_id: NodeId,
}

impl Default for AstTree {
    fn default() -> Self {
        Self::new()
    }
}

impl AstTree {
    pub fn new() -> Self {
        let root = AstNode::new(0, AstNodeKind::Root, "root");
        let mut index = HashMap::new();
        index.insert(0, 0);
        Self {
            nodes: vec![root],
            index,
            parent_index: HashMap::new(),
            root_id: 0,
            next_id: 1,
        }
    }

    /// Add a node, returns its ID
    pub fn add_node(&mut self, kind: AstNodeKind, label: &str, parent_id: NodeId) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        let node = AstNode::new(id, kind, label);
        let idx = self.nodes.len();
        self.nodes.push(node);
        self.index.insert(id, idx);
        self.parent_index.insert(id, parent_id);
        // Add to parent's children
        if let Some(parent) = self.get_node_mut(parent_id) {
            parent.children.push(id);
        }
        id
    }

    /// Add a node with a value
    pub fn add_node_with_value(
        &mut self,
        kind: AstNodeKind,
        label: &str,
        value: NodeValue,
        parent_id: NodeId,
    ) -> NodeId {
        let id = self.add_node(kind, label, parent_id);
        if let Some(node) = self.get_node_mut(id) {
            node.value = value;
        }
        id
    }

    /// Get node by ID — O(1) via HashMap index
    pub fn get_node(&self, id: NodeId) -> Option<&AstNode> {
        self.index.get(&id).map(|&idx| &self.nodes[idx])
    }

    /// Get mutable node by ID — O(1) via HashMap index
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut AstNode> {
        self.index.get(&id).map(|&idx| &mut self.nodes[idx])
    }

    /// Root node ID
    pub fn root_id(&self) -> NodeId {
        self.root_id
    }

    /// Total node count
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[AstNode] {
        &self.nodes
    }

    /// Find parent of a node — O(1) via parent index
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.parent_index.get(&id).copied()
    }

    /// Remove a node and all its descendants
    pub fn remove_subtree(&mut self, id: NodeId) {
        // Collect IDs to remove into a Vec first, then build a HashSet for O(1) membership.
        let mut to_remove_vec = Vec::new();
        self.collect_subtree(id, &mut to_remove_vec);

        // Build HashSet for O(1) membership test used in retain() below.
        let to_remove: HashMap<NodeId, ()> =
            to_remove_vec.iter().map(|&rid| (rid, ())).collect();

        // Remove from parent's children
        if let Some(parent_id) = self.parent_of(id) {
            if let Some(parent) = self.get_node_mut(parent_id) {
                parent.children.retain(|&c| c != id);
            }
        }

        // Remove from parent_index — O(1) per entry
        for &rid in to_remove.keys() {
            self.parent_index.remove(&rid);
        }

        // Remove nodes — O(1) per node via HashMap lookup instead of O(n) Vec scan
        self.nodes.retain(|n| !to_remove.contains_key(&n.id));
        self.index.clear();
        for (idx, node) in self.nodes.iter().enumerate() {
            self.index.insert(node.id, idx);
        }
    }

    fn collect_subtree(&self, id: NodeId, result: &mut Vec<NodeId>) {
        result.push(id);
        if let Some(node) = self.get_node(id) {
            for &child_id in &node.children {
                self.collect_subtree(child_id, result);
            }
        }
    }

    /// Compute Merkle hash of a subtree (FNV-1a)
    pub fn subtree_hash(&self, id: NodeId) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        self.hash_node(id, &mut h);
        h
    }

    fn hash_node(&self, id: NodeId, h: &mut u64) {
        if let Some(node) = self.get_node(id) {
            // Hash kind + label
            *h ^= node.kind as u64;
            *h = h.wrapping_mul(0x100000001b3);
            for &b in node.label.as_bytes() {
                *h ^= b as u64;
                *h = h.wrapping_mul(0x100000001b3);
            }
            // Hash children recursively
            for &child_id in &node.children {
                self.hash_node(child_id, h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::format;

    #[test]
    fn test_tree_construction() {
        let mut tree = AstTree::new();
        let sphere = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        let _radius = tree.add_node_with_value(
            AstNodeKind::Parameter,
            "radius",
            NodeValue::Float(1.0),
            sphere,
        );
        assert_eq!(tree.node_count(), 3); // root + sphere + radius
    }

    #[test]
    fn test_parent_lookup() {
        let mut tree = AstTree::new();
        let child = tree.add_node(AstNodeKind::Group, "group1", 0);
        assert_eq!(tree.parent_of(child), Some(0));
    }

    #[test]
    fn test_remove_subtree() {
        let mut tree = AstTree::new();
        let group = tree.add_node(AstNodeKind::Group, "g", 0);
        let _child = tree.add_node(AstNodeKind::Primitive, "s", group);
        assert_eq!(tree.node_count(), 3);
        tree.remove_subtree(group);
        assert_eq!(tree.node_count(), 1); // only root left
    }

    #[test]
    fn test_subtree_hash_differs() {
        let mut tree1 = AstTree::new();
        tree1.add_node(AstNodeKind::Primitive, "sphere", 0);
        let mut tree2 = AstTree::new();
        tree2.add_node(AstNodeKind::Primitive, "box", 0);
        assert_ne!(tree1.subtree_hash(0), tree2.subtree_hash(0));
    }

    #[test]
    fn test_subtree_hash_same() {
        let mut tree1 = AstTree::new();
        tree1.add_node(AstNodeKind::Primitive, "sphere", 0);
        let mut tree2 = AstTree::new();
        tree2.add_node(AstNodeKind::Primitive, "sphere", 0);
        assert_eq!(tree1.subtree_hash(0), tree2.subtree_hash(0));
    }

    #[test]
    fn test_get_node_o1_lookup() {
        let mut tree = AstTree::new();
        let id = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        // Verify O(1) lookup returns correct node
        let node = tree.get_node(id).expect("node must be found");
        assert_eq!(node.id, id);
        assert_eq!(node.label, "sphere");
        // Non-existent ID returns None
        assert!(tree.get_node(9999).is_none());
    }

    // ── New tests ──────────────────────────────────────────────────────

    #[test]
    fn test_root_is_always_node_zero() {
        let tree = AstTree::new();
        assert_eq!(tree.root_id(), 0);
        let root = tree.get_node(0).expect("root must exist");
        assert_eq!(root.kind, AstNodeKind::Root);
    }

    #[test]
    fn test_new_tree_has_one_node() {
        let tree = AstTree::new();
        assert_eq!(tree.node_count(), 1);
    }

    #[test]
    fn test_add_node_increments_count() {
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Primitive, "a", 0);
        tree.add_node(AstNodeKind::Primitive, "b", 0);
        assert_eq!(tree.node_count(), 3);
    }

    #[test]
    fn test_add_node_with_value_stores_value() {
        let mut tree = AstTree::new();
        let id = tree.add_node_with_value(
            AstNodeKind::Parameter,
            "radius",
            NodeValue::Float(3.14),
            0,
        );
        assert_eq!(tree.get_node(id).unwrap().value, NodeValue::Float(3.14));
    }

    #[test]
    fn test_root_children_updated_on_add() {
        let mut tree = AstTree::new();
        let id = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        assert!(tree.get_node(0).unwrap().children.contains(&id));
    }

    #[test]
    fn test_get_node_mut_modifies_in_place() {
        let mut tree = AstTree::new();
        let id = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        tree.get_node_mut(id).unwrap().label = String::from("box");
        assert_eq!(tree.get_node(id).unwrap().label, "box");
    }

    #[test]
    fn test_nodes_slice_length() {
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Group, "g", 0);
        tree.add_node(AstNodeKind::Primitive, "s", 0);
        assert_eq!(tree.nodes().len(), 3);
    }

    #[test]
    fn test_remove_subtree_deep() {
        let mut tree = AstTree::new();
        let g = tree.add_node(AstNodeKind::Group, "g", 0);
        let c1 = tree.add_node(AstNodeKind::Primitive, "c1", g);
        tree.add_node(AstNodeKind::Parameter, "p", c1);
        // root -> g -> c1 -> p  (4 nodes)
        assert_eq!(tree.node_count(), 4);
        tree.remove_subtree(g);
        assert_eq!(tree.node_count(), 1);
        assert!(tree.get_node(g).is_none());
    }

    #[test]
    fn test_remove_subtree_updates_parent_children() {
        let mut tree = AstTree::new();
        let c = tree.add_node(AstNodeKind::Primitive, "c", 0);
        tree.remove_subtree(c);
        assert!(!tree.get_node(0).unwrap().children.contains(&c));
    }

    #[test]
    fn test_node_kind_roundtrip_from_u8() {
        let cases: &[(u8, AstNodeKind)] = &[
            (0, AstNodeKind::Root),
            (1, AstNodeKind::CsgOp),
            (2, AstNodeKind::Primitive),
            (3, AstNodeKind::Transform),
            (4, AstNodeKind::Parameter),
            (5, AstNodeKind::Group),
            (6, AstNodeKind::Material),
            (7, AstNodeKind::Keyframe),
            (255, AstNodeKind::Custom),
            (42, AstNodeKind::Custom), // unknown -> Custom
        ];
        for &(byte, expected) in cases {
            assert_eq!(AstNodeKind::from_u8(byte), expected);
        }
    }

    #[test]
    fn test_node_value_serialized_size() {
        assert_eq!(NodeValue::None.serialized_size(), 1);
        assert_eq!(NodeValue::Int(0).serialized_size(), 9);
        assert_eq!(NodeValue::Float(0.0).serialized_size(), 9);
        assert_eq!(NodeValue::Text(String::from("hi")).serialized_size(), 5);  // 3+2
        assert_eq!(NodeValue::Ident(String::from("abc")).serialized_size(), 6); // 3+3
        assert_eq!(NodeValue::Bytes(vec![1, 2]).serialized_size(), 5);          // 3+2
    }

    #[test]
    fn test_subtree_hash_deterministic() {
        let mut t1 = AstTree::new();
        t1.add_node(AstNodeKind::Primitive, "sphere", 0);
        let mut t2 = AstTree::new();
        t2.add_node(AstNodeKind::Primitive, "sphere", 0);
        assert_eq!(t1.subtree_hash(0), t2.subtree_hash(0));
    }

    #[test]
    fn test_default_tree_equals_new() {
        let t1 = AstTree::new();
        let t2 = AstTree::default();
        assert_eq!(t1.node_count(), t2.node_count());
        assert_eq!(t1.root_id(), t2.root_id());
    }

    #[test]
    fn test_ast_node_builder_methods() {
        let node = AstNode::new(42, AstNodeKind::Group, "grp")
            .with_value(NodeValue::Int(7))
            .with_children(vec![1, 2, 3]);
        assert_eq!(node.id, 42);
        assert_eq!(node.value, NodeValue::Int(7));
        assert_eq!(node.children, vec![1, 2, 3]);
    }

    #[test]
    fn test_parent_of_root_is_none() {
        let tree = AstTree::new();
        // Root has no parent
        assert!(tree.parent_of(0).is_none());
    }

    #[test]
    fn test_parent_of_nonexistent_is_none() {
        let tree = AstTree::new();
        assert!(tree.parent_of(9999).is_none());
    }

    // ── HashMap O(1) optimisation tests ────────────────────────────────

    /// remove_subtree on a wide tree (many siblings under one parent).
    /// Verifies the HashSet-based retain path is correct when many IDs
    /// need to be checked.
    #[test]
    fn test_remove_subtree_wide_parent() {
        let mut tree = AstTree::new();
        let group = tree.add_node(AstNodeKind::Group, "g", 0);
        // Add 20 children to the group
        let mut child_ids = Vec::new();
        for i in 0u32..20 {
            let label = format!("child_{i}");
            let c = tree.add_node(AstNodeKind::Primitive, &label, group);
            child_ids.push(c);
        }
        // root + group + 20 children = 22
        assert_eq!(tree.node_count(), 22);

        // Remove the group and all its 20 children
        tree.remove_subtree(group);

        // Only root should remain
        assert_eq!(tree.node_count(), 1);
        assert!(tree.get_node(group).is_none());
        for &c in &child_ids {
            assert!(tree.get_node(c).is_none());
        }
    }

    /// remove_subtree on a deeply nested tree (long chain).
    /// Exercises the recursive collect_subtree + HashSet path.
    #[test]
    fn test_remove_subtree_deep_chain() {
        let mut tree = AstTree::new();
        let mut parent = 0u32; // root
        let mut ids = Vec::new();
        for i in 0u32..15 {
            let label = format!("level_{i}");
            let id = tree.add_node(AstNodeKind::Group, &label, parent);
            ids.push(id);
            parent = id;
        }
        // root + 15 nodes = 16
        assert_eq!(tree.node_count(), 16);

        // Remove from the very top (first child of root)
        tree.remove_subtree(ids[0]);

        // Only root remains
        assert_eq!(tree.node_count(), 1);
        for &id in &ids {
            assert!(tree.get_node(id).is_none(), "node {id} should be removed");
        }
    }

    /// Removing a leaf (no children) via remove_subtree should only remove
    /// that single node — verifying no over-removal by the HashSet path.
    #[test]
    fn test_remove_subtree_leaf_only() {
        let mut tree = AstTree::new();
        let a = tree.add_node(AstNodeKind::Primitive, "a", 0);
        let b = tree.add_node(AstNodeKind::Primitive, "b", 0);
        let c = tree.add_node(AstNodeKind::Primitive, "c", 0);

        // Remove the middle sibling
        tree.remove_subtree(b);

        assert_eq!(tree.node_count(), 3); // root + a + c
        assert!(tree.get_node(a).is_some());
        assert!(tree.get_node(b).is_none());
        assert!(tree.get_node(c).is_some());
        // Root should still have only a and c as children
        let root = tree.get_node(0).unwrap();
        assert!(!root.children.contains(&b));
        assert!(root.children.contains(&a));
        assert!(root.children.contains(&c));
    }

    /// After remove_subtree the HashMap index must remain consistent —
    /// get_node must still work for every surviving node.
    #[test]
    fn test_remove_subtree_index_consistent_after_removal() {
        let mut tree = AstTree::new();
        let g = tree.add_node(AstNodeKind::Group, "g", 0);
        let keep1 = tree.add_node(AstNodeKind::Primitive, "k1", 0);
        let keep2 = tree.add_node(AstNodeKind::Primitive, "k2", 0);
        let c = tree.add_node(AstNodeKind::Primitive, "c", g);
        let _gc = tree.add_node(AstNodeKind::Parameter, "p", c);

        // Remove group and its descendants
        tree.remove_subtree(g);

        // root, keep1, keep2 survive
        assert_eq!(tree.node_count(), 3);
        assert!(tree.get_node(0).is_some());
        assert!(tree.get_node(keep1).is_some());
        assert!(tree.get_node(keep2).is_some());
    }

    /// remove_subtree followed by add_node should assign a fresh ID and
    /// work correctly, proving the index rebuild after remove is correct.
    #[test]
    fn test_add_node_after_remove_subtree() {
        let mut tree = AstTree::new();
        let old = tree.add_node(AstNodeKind::Primitive, "old", 0);
        tree.remove_subtree(old);
        assert_eq!(tree.node_count(), 1);

        // Adding a new node must succeed
        let new_id = tree.add_node(AstNodeKind::Primitive, "new", 0);
        assert_eq!(tree.node_count(), 2);
        assert!(tree.get_node(new_id).is_some());
        assert_eq!(tree.get_node(new_id).unwrap().label, "new");
    }

    /// Removing a sibling group should NOT remove unrelated siblings,
    /// verifying the HashSet contains exactly the right IDs.
    #[test]
    fn test_remove_subtree_does_not_affect_siblings() {
        let mut tree = AstTree::new();
        let g1 = tree.add_node(AstNodeKind::Group, "g1", 0);
        let g2 = tree.add_node(AstNodeKind::Group, "g2", 0);
        let _c1 = tree.add_node(AstNodeKind::Primitive, "c1", g1);
        let c2 = tree.add_node(AstNodeKind::Primitive, "c2", g2);

        // Remove g1 subtree (g1 + c1)
        tree.remove_subtree(g1);

        // g2 and c2 must be untouched
        assert!(tree.get_node(g2).is_some());
        assert!(tree.get_node(c2).is_some());
        assert_eq!(tree.node_count(), 3); // root + g2 + c2
    }

    /// parent_index cleanup: after removing a subtree, parent_of for any
    /// removed node must return None (HashSet cleanup verification).
    #[test]
    fn test_remove_subtree_clears_parent_index() {
        let mut tree = AstTree::new();
        let g = tree.add_node(AstNodeKind::Group, "g", 0);
        let c = tree.add_node(AstNodeKind::Primitive, "c", g);
        let gc = tree.add_node(AstNodeKind::Parameter, "gc", c);

        tree.remove_subtree(g);

        assert!(tree.parent_of(g).is_none());
        assert!(tree.parent_of(c).is_none());
        assert!(tree.parent_of(gc).is_none());
    }

    /// Subtree hash should remain deterministic even after remove_subtree
    /// modifies the tree, verifying the index rebuild produces a stable hash.
    #[test]
    fn test_subtree_hash_stable_after_remove() {
        let mut tree = AstTree::new();
        let keep = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        let drop = tree.add_node(AstNodeKind::Group, "temp", 0);
        tree.add_node(AstNodeKind::Primitive, "child", drop);
        let _ = keep;

        let hash_before = tree.subtree_hash(0);

        // Remove the temporary group
        tree.remove_subtree(drop);

        // Build a fresh tree with only the kept node to compare
        let mut ref_tree = AstTree::new();
        ref_tree.add_node(AstNodeKind::Primitive, "sphere", 0);

        // After removal the tree should match the reference
        assert_eq!(tree.subtree_hash(0), ref_tree.subtree_hash(0));
        assert_ne!(tree.subtree_hash(0), hash_before);
    }

    /// Multiple sequential remove_subtree calls must each leave a consistent
    /// index — stress test for the HashMap-based optimisation.
    #[test]
    fn test_multiple_sequential_removals() {
        let mut tree = AstTree::new();
        let mut ids = Vec::new();
        for i in 0u32..10 {
            let id = tree.add_node(AstNodeKind::Primitive, &format!("n{i}"), 0);
            ids.push(id);
        }
        assert_eq!(tree.node_count(), 11); // root + 10

        // Remove every other node
        for i in (0..10).step_by(2) {
            tree.remove_subtree(ids[i]);
        }

        // 5 removed, 5 remain + root = 6
        assert_eq!(tree.node_count(), 6);
        for i in 0..10 {
            if i % 2 == 0 {
                assert!(tree.get_node(ids[i]).is_none(), "ids[{i}] should be gone");
            } else {
                assert!(tree.get_node(ids[i]).is_some(), "ids[{i}] should survive");
            }
        }
    }

    /// Removing the root's only child makes root a leaf; subsequent
    /// add_node must still attach under root correctly.
    #[test]
    fn test_remove_only_child_then_readd() {
        let mut tree = AstTree::new();
        let child = tree.add_node(AstNodeKind::Primitive, "only", 0);
        tree.remove_subtree(child);
        assert_eq!(tree.node_count(), 1);
        assert!(tree.get_node(0).unwrap().children.is_empty());

        let new_child = tree.add_node(AstNodeKind::Primitive, "new", 0);
        assert_eq!(tree.node_count(), 2);
        assert!(tree.get_node(0).unwrap().children.contains(&new_child));
    }
}
