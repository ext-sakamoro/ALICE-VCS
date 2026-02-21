//! Content-addressed snapshot store
//!
//! Merkle DAG storage for AST snapshots. Each snapshot is identified
//! by its content hash (FNV-1a). Deduplication is automatic.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::{vec::Vec, collections::BTreeMap as HashMap};
#[cfg(feature = "std")]
use std::collections::HashMap;

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

/// Content-addressed snapshot store (O(1) lookup via HashMap)
pub struct SnapshotStore {
    snapshots: HashMap<Hash, Snapshot>,
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
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

    /// List all stored snapshot hashes.
    pub fn all_hashes(&self) -> Vec<Hash> {
        self.snapshots.keys().copied().collect()
    }

    /// Remove a snapshot by hash. Returns `true` if it existed.
    pub fn remove(&mut self, hash: Hash) -> bool {
        self.snapshots.remove(&hash).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNodeKind, AstTree};
    #[cfg(not(feature = "std"))]
    use alloc::vec;
    #[cfg(not(feature = "std"))]
    use alloc::format;

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

    // ── New tests ──────────────────────────────────────────────────────

    #[test]
    fn test_store_default_is_empty() {
        let store = SnapshotStore::default();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_store_get_nonexistent_returns_none() {
        let store = SnapshotStore::new();
        assert!(store.get(0xDEAD_BEEF).is_none());
    }

    #[test]
    fn test_store_contains_after_store() {
        let mut store = SnapshotStore::new();
        let tree = AstTree::new();
        let hash = store.store(&tree, vec![]);
        assert!(store.contains(hash));
    }

    #[test]
    fn test_store_remove_returns_true_when_present() {
        let mut store = SnapshotStore::new();
        let tree = AstTree::new();
        let hash = store.store(&tree, vec![]);
        assert!(store.remove(hash));
        assert!(!store.contains(hash));
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_store_remove_returns_false_when_absent() {
        let mut store = SnapshotStore::new();
        assert!(!store.remove(0xABCD));
    }

    #[test]
    fn test_all_hashes_matches_len() {
        let mut store = SnapshotStore::new();
        let mut tree = AstTree::new();
        let h0 = store.store(&tree, vec![]);
        tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        let h1 = store.store(&tree, vec![h0]);
        let hashes = store.all_hashes();
        assert_eq!(hashes.len(), store.len());
        assert!(hashes.contains(&h0));
        assert!(hashes.contains(&h1));
    }

    #[test]
    fn test_parents_of_root_snapshot_is_empty() {
        let mut store = SnapshotStore::new();
        let tree = AstTree::new();
        let hash = store.store(&tree, vec![]);
        assert_eq!(store.parents(hash).unwrap(), &[] as &[u64]);
    }

    #[test]
    fn test_store_multiple_snapshots() {
        let mut store = SnapshotStore::new();
        for i in 0u32..5 {
            let mut tree = AstTree::new();
            tree.add_node(AstNodeKind::Primitive, &format!("n{i}"), 0);
            store.store(&tree, vec![]);
        }
        assert_eq!(store.len(), 5);
    }
}
