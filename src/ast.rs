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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    /// Find parent of a node
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        for node in &self.nodes {
            if node.children.contains(&id) {
                return Some(node.id);
            }
        }
        None
    }

    /// Remove a node and all its descendants
    pub fn remove_subtree(&mut self, id: NodeId) {
        // Collect IDs to remove
        let mut to_remove = Vec::new();
        self.collect_subtree(id, &mut to_remove);

        // Remove from parent's children
        if let Some(parent_id) = self.parent_of(id) {
            if let Some(parent) = self.get_node_mut(parent_id) {
                parent.children.retain(|&c| c != id);
            }
        }

        // Remove nodes and rebuild index
        self.nodes.retain(|n| !to_remove.contains(&n.id));
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
}
