//! Content-addressed snapshot store
//!
//! Merkle DAG storage for AST snapshots. Each snapshot is identified
//! by its content hash (FNV-1a). Deduplication is automatic.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec, collections::BTreeMap};
#[cfg(feature = "std")]
use std::collections::BTreeMap;

use crate::ast::AstTree;

/// Content hash (FNV-1a 64-bit)
pub type Hash = u64;

/// Snapshot entry in the store
#[derive(Debug, Clone)]
struct Snapshot {
    /// The AST tree
    tree: AstTree,
    /// Parent snapshot hashes
    parents: Vec<Hash>,
}

/// Content-addressed snapshot store
pub struct SnapshotStore {
    snapshots: BTreeMap<Hash, Snapshot>,
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: BTreeMap::new(),
        }
    }

    /// Store a snapshot, returns its content hash
    pub fn store(&mut self, tree: &AstTree, parents: Vec<Hash>) -> Hash {
        let hash = tree.subtree_hash(tree.root_id());
        // Include parents in hash for unique commit identity
        let mut commit_hash = hash;
        for &p in &parents {
            commit_hash ^= p;
            commit_hash = commit_hash.wrapping_mul(0x100000001b3);
        }

        self.snapshots.insert(
            commit_hash,
            Snapshot {
                tree: tree.clone(),
                parents,
            },
        );
        commit_hash
    }

    /// Retrieve a snapshot by hash
    pub fn get(&self, hash: Hash) -> Option<&AstTree> {
        self.snapshots.get(&hash).map(|s| &s.tree)
    }

    /// Get parent hashes
    pub fn parents(&self, hash: Hash) -> Option<&[Hash]> {
        self.snapshots.get(&hash).map(|s| s.parents.as_slice())
    }

    /// Check if hash exists
    pub fn contains(&self, hash: Hash) -> bool {
        self.snapshots.contains_key(&hash)
    }

    /// Total stored snapshots
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Is the store empty?
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNodeKind, AstTree};

    #[test]
    fn test_store_and_retrieve() {
        let mut store = SnapshotStore::new();
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Primitive, "sphere", 0);

        let hash = store.store(&tree, vec![]);
        assert!(store.contains(hash));
        let retrieved = store.get(hash).unwrap();
        assert_eq!(retrieved.node_count(), tree.node_count());
    }

    #[test]
    fn test_parent_tracking() {
        let mut store = SnapshotStore::new();
        let mut tree1 = AstTree::new();
        tree1.add_node(AstNodeKind::Primitive, "sphere", 0);
        let h1 = store.store(&tree1, vec![]);

        let mut tree2 = AstTree::new();
        tree2.add_node(AstNodeKind::Primitive, "box", 0);
        let h2 = store.store(&tree2, vec![h1]);

        let parents = store.parents(h2).unwrap();
        assert_eq!(parents, &[h1]);
    }

    #[test]
    fn test_store_count() {
        let mut store = SnapshotStore::new();
        assert!(store.is_empty());

        let tree = AstTree::new();
        store.store(&tree, vec![]);
        assert_eq!(store.len(), 1);
    }
}
