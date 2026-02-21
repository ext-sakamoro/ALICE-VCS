//! Mark-sweep garbage collection for the snapshot store
//!
//! Identifies unreachable snapshots (not reachable from any branch HEAD)
//! and removes them, reclaiming storage.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeSet as HashSet;
#[cfg(feature = "std")]
use std::collections::HashSet;

use crate::store::{Hash, SnapshotStore};

// ── GC Result ──────────────────────────────────────────────────────────

/// Statistics from a garbage collection run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcResult {
    /// Number of snapshots retained (reachable from roots).
    pub retained: usize,
    /// Number of snapshots collected (removed).
    pub collected: usize,
    /// Total snapshots before GC.
    pub total_before: usize,
}

impl GcResult {
    /// True if any snapshots were collected.
    #[inline]
    pub fn did_collect(&self) -> bool {
        self.collected > 0
    }
}

// ── Mark-Sweep GC ─────────────────────────────────────────────────────

/// Run mark-sweep garbage collection on the snapshot store.
///
/// Starting from the given `root_hashes` (typically branch HEAD hashes),
/// walks the parent DAG to find all reachable snapshots.  Unreachable
/// snapshots are removed from the store.
///
/// # Algorithm
///
/// 1. **Mark**: BFS from each root hash, following parent links.
///    All visited hashes are added to the reachable set.
/// 2. **Sweep**: Remove all snapshots not in the reachable set.
pub fn collect_garbage(store: &mut SnapshotStore, root_hashes: &[Hash]) -> GcResult {
    let all_hashes = store.all_hashes();
    let total_before = all_hashes.len();

    // Mark phase: BFS from roots through parent links
    let reachable = mark(store, root_hashes);

    // Sweep phase: remove unreachable snapshots
    let mut collected = 0;
    for hash in &all_hashes {
        if !reachable.contains(hash) {
            store.remove(*hash);
            collected += 1;
        }
    }

    GcResult {
        retained: total_before - collected,
        collected,
        total_before,
    }
}

/// Mark all reachable snapshots via BFS from root hashes.
fn mark(store: &SnapshotStore, root_hashes: &[Hash]) -> HashSet<Hash> {
    let mut reachable = HashSet::new();
    let mut queue: Vec<Hash> = Vec::new();

    // Seed with roots
    for &root in root_hashes {
        if store.contains(root) && !reachable.contains(&root) {
            reachable.insert(root);
            queue.push(root);
        }
    }

    // BFS: walk parent links
    while let Some(hash) = queue.pop() {
        if let Some(parents) = store.parents(hash) {
            for &parent_hash in parents {
                if store.contains(parent_hash) && !reachable.contains(&parent_hash) {
                    reachable.insert(parent_hash);
                    queue.push(parent_hash);
                }
            }
        }
    }

    reachable
}

/// Dry-run: compute what would be collected without actually removing.
pub fn dry_run(store: &SnapshotStore, root_hashes: &[Hash]) -> GcResult {
    let total_before = store.len();
    let reachable = mark(store, root_hashes);
    let retained = reachable.len();
    GcResult {
        retained,
        collected: total_before - retained,
        total_before,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNodeKind, AstTree};
    #[cfg(not(feature = "std"))]
    use alloc::vec;

    fn make_tree(label: &str) -> AstTree {
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Primitive, label, 0);
        tree
    }

    #[test]
    fn gc_empty_store() {
        let mut store = SnapshotStore::new();
        let result = collect_garbage(&mut store, &[]);
        assert_eq!(result.total_before, 0);
        assert_eq!(result.retained, 0);
        assert_eq!(result.collected, 0);
        assert!(!result.did_collect());
    }

    #[test]
    fn gc_all_reachable() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("sphere");
        let h1 = store.store(&t1, vec![]);

        let t2 = make_tree("box");
        let h2 = store.store(&t2, vec![h1]);

        // Both reachable from h2 (h2 → h1)
        let result = collect_garbage(&mut store, &[h2]);
        assert_eq!(result.retained, 2);
        assert_eq!(result.collected, 0);
        assert!(!result.did_collect());
    }

    #[test]
    fn gc_collects_unreachable() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("sphere");
        let h1 = store.store(&t1, vec![]);

        let t2 = make_tree("box");
        let h2 = store.store(&t2, vec![]);

        let t3 = make_tree("cylinder");
        let h3 = store.store(&t3, vec![h2]);

        // Root = h3. h3 → h2 is reachable. h1 is orphaned.
        let result = collect_garbage(&mut store, &[h3]);
        assert_eq!(result.retained, 2);
        assert_eq!(result.collected, 1);
        assert!(result.did_collect());

        assert!(!store.contains(h1));
        assert!(store.contains(h2));
        assert!(store.contains(h3));
    }

    #[test]
    fn gc_multiple_roots() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("a");
        let h1 = store.store(&t1, vec![]);

        let t2 = make_tree("b");
        let h2 = store.store(&t2, vec![]);

        let t3 = make_tree("orphan");
        let h3 = store.store(&t3, vec![]);

        // Two roots: h1 and h2 are reachable, h3 is not
        let result = collect_garbage(&mut store, &[h1, h2]);
        assert_eq!(result.retained, 2);
        assert_eq!(result.collected, 1);
        assert!(!store.contains(h3));
    }

    #[test]
    fn gc_chain_reachability() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("v1");
        let h1 = store.store(&t1, vec![]);

        let t2 = make_tree("v2");
        let h2 = store.store(&t2, vec![h1]);

        let t3 = make_tree("v3");
        let h3 = store.store(&t3, vec![h2]);

        let t4 = make_tree("v4");
        let h4 = store.store(&t4, vec![h3]);

        // Root at h4 → h3 → h2 → h1, all reachable
        let result = collect_garbage(&mut store, &[h4]);
        assert_eq!(result.retained, 4);
        assert_eq!(result.collected, 0);
    }

    #[test]
    fn gc_diamond_dag() {
        let mut store = SnapshotStore::new();
        let t_base = make_tree("base");
        let h_base = store.store(&t_base, vec![]);

        let t_a = make_tree("branch_a");
        let h_a = store.store(&t_a, vec![h_base]);

        let t_b = make_tree("branch_b");
        let h_b = store.store(&t_b, vec![h_base]);

        // Merge commit with two parents
        let t_merge = make_tree("merge");
        let h_merge = store.store(&t_merge, vec![h_a, h_b]);

        // Root at merge → reaches a, b, base
        let result = collect_garbage(&mut store, &[h_merge]);
        assert_eq!(result.retained, 4);
        assert_eq!(result.collected, 0);
    }

    #[test]
    fn gc_nonexistent_root_ignored() {
        let mut store = SnapshotStore::new();
        let t = make_tree("x");
        let h = store.store(&t, vec![]);

        // Root 0xDEAD doesn't exist in store — should not panic
        let result = collect_garbage(&mut store, &[0xDEAD]);
        assert_eq!(result.collected, 1); // h is unreachable
        assert!(!store.contains(h));
    }

    #[test]
    fn dry_run_does_not_modify() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("keep");
        let h1 = store.store(&t1, vec![]);

        let t2 = make_tree("orphan");
        let _h2 = store.store(&t2, vec![]);

        let result = dry_run(&store, &[h1]);
        assert_eq!(result.collected, 1);
        // Store is unmodified
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn gc_single_snapshot_no_root() {
        let mut store = SnapshotStore::new();
        let t = make_tree("lonely");
        store.store(&t, vec![]);

        // No roots → everything is garbage
        let result = collect_garbage(&mut store, &[]);
        assert_eq!(result.collected, 1);
        assert!(store.is_empty());
    }

    #[test]
    fn gc_result_did_collect() {
        let r1 = GcResult {
            retained: 5,
            collected: 0,
            total_before: 5,
        };
        assert!(!r1.did_collect());

        let r2 = GcResult {
            retained: 3,
            collected: 2,
            total_before: 5,
        };
        assert!(r2.did_collect());
    }

    #[test]
    fn gc_preserves_parent_links() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("v1");
        let h1 = store.store(&t1, vec![]);

        let t2 = make_tree("v2");
        let h2 = store.store(&t2, vec![h1]);

        collect_garbage(&mut store, &[h2]);

        // Parent links should still be intact
        let parents = store.parents(h2).unwrap();
        assert_eq!(parents, &[h1]);
    }

    // ── New tests ──────────────────────────────────────────────────────

    #[test]
    fn gc_total_before_is_correct() {
        let mut store = SnapshotStore::new();
        for label in &["a", "b", "c"] {
            let t = make_tree(label);
            store.store(&t, vec![]);
        }
        let result = collect_garbage(&mut store, &[]);
        assert_eq!(result.total_before, 3);
        assert_eq!(result.collected, 3);
    }

    #[test]
    fn gc_retained_plus_collected_equals_total() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("keep");
        let h1 = store.store(&t1, vec![]);
        let t2 = make_tree("drop");
        store.store(&t2, vec![]);

        let result = collect_garbage(&mut store, &[h1]);
        assert_eq!(result.retained + result.collected, result.total_before);
    }

    #[test]
    fn dry_run_matches_gc_stats() {
        let mut store = SnapshotStore::new();
        let t1 = make_tree("keep");
        let h1 = store.store(&t1, vec![]);
        let t2 = make_tree("orphan");
        store.store(&t2, vec![]);

        // dry_run should predict the same numbers as a real GC
        let dry = dry_run(&store, &[h1]);
        assert_eq!(dry.retained, 1);
        assert_eq!(dry.collected, 1);
        assert_eq!(dry.total_before, 2);
    }

    #[test]
    fn gc_idempotent_when_all_reachable() {
        let mut store = SnapshotStore::new();
        let t = make_tree("x");
        let h = store.store(&t, vec![]);

        let r1 = collect_garbage(&mut store, &[h]);
        assert!(!r1.did_collect());

        // Running again on the same store should be a no-op
        let r2 = collect_garbage(&mut store, &[h]);
        assert!(!r2.did_collect());
        assert_eq!(r2.retained, 1);
    }

    #[test]
    fn gc_duplicate_root_hashes_handled() {
        let mut store = SnapshotStore::new();
        let t = make_tree("dup");
        let h = store.store(&t, vec![]);

        // Passing same root twice should not double-count
        let result = collect_garbage(&mut store, &[h, h]);
        assert_eq!(result.retained, 1);
        assert_eq!(result.collected, 0);
    }
}
