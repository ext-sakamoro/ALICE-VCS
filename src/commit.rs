//! Commit and branch model
//!
//! Git-like commit/branch abstraction over the AST snapshot store.
//! Commits are immutable, branches are movable pointers.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec, collections::BTreeMap};
#[cfg(feature = "std")]
use std::collections::BTreeMap;

use crate::ast::AstTree;
use crate::diff::{diff_trees, apply_patch, DiffOp};
use crate::merge::{merge_patches, MergeResult};
use crate::store::{Hash, SnapshotStore};

/// A commit in the history DAG
#[derive(Debug, Clone)]
pub struct Commit {
    /// Content hash
    pub hash: Hash,
    /// Parent commit hash(es)
    pub parents: Vec<Hash>,
    /// Commit message
    pub message: String,
    /// Author
    pub author: String,
    /// Diff operations from parent (stored for small patches)
    pub patch: Vec<DiffOp>,
}

/// Branch pointer
#[derive(Debug, Clone)]
pub struct Branch {
    /// Branch name
    pub name: String,
    /// Points to current commit hash
    pub head: Hash,
}

/// Repository — manages branches, commits, and snapshots
pub struct Repository {
    /// Snapshot store
    store: SnapshotStore,
    /// Commits indexed by hash
    commits: BTreeMap<Hash, Commit>,
    /// Branches
    branches: BTreeMap<String, Branch>,
    /// Current branch name
    current_branch: String,
}

impl Default for Repository {
    fn default() -> Self {
        Self::new()
    }
}

impl Repository {
    pub fn new() -> Self {
        let mut repo = Self {
            store: SnapshotStore::new(),
            commits: BTreeMap::new(),
            branches: BTreeMap::new(),
            current_branch: String::from("main"),
        };

        // Create initial empty commit
        let tree = AstTree::new();
        let hash = repo.store.store(&tree, vec![]);
        let commit = Commit {
            hash,
            parents: vec![],
            message: String::from("initial commit"),
            author: String::from("system"),
            patch: vec![],
        };
        repo.commits.insert(hash, commit);
        repo.branches.insert(
            String::from("main"),
            Branch {
                name: String::from("main"),
                head: hash,
            },
        );
        repo
    }

    /// Commit a new tree state
    pub fn commit(&mut self, tree: &AstTree, message: &str, author: &str) -> Hash {
        let parent_hash = self.head_hash();
        let parent_tree = self.store.get(parent_hash).cloned();

        let patch = if let Some(ref parent) = parent_tree {
            diff_trees(parent, tree)
        } else {
            vec![]
        };

        let hash = self.store.store(tree, vec![parent_hash]);
        let commit = Commit {
            hash,
            parents: vec![parent_hash],
            message: String::from(message),
            author: String::from(author),
            patch,
        };
        self.commits.insert(hash, commit);

        // Advance branch
        if let Some(branch) = self.branches.get_mut(&self.current_branch) {
            branch.head = hash;
        }

        hash
    }

    /// Create a new branch at current HEAD
    pub fn create_branch(&mut self, name: &str) {
        let head = self.head_hash();
        self.branches.insert(
            String::from(name),
            Branch {
                name: String::from(name),
                head,
            },
        );
    }

    /// Switch to a branch
    pub fn checkout(&mut self, name: &str) -> bool {
        if self.branches.contains_key(name) {
            self.current_branch = String::from(name);
            true
        } else {
            false
        }
    }

    /// Merge another branch into current
    pub fn merge(&mut self, other_branch: &str) -> Option<MergeResult> {
        let current_hash = self.head_hash();
        let other_hash = self.branches.get(other_branch)?.head;

        // Find common ancestor (simplified: assume parent of current)
        let current_commit = self.commits.get(&current_hash)?;
        let ancestor_hash = current_commit.parents.first().copied()?;

        let ancestor_tree = self.store.get(ancestor_hash)?.clone();
        let current_tree = self.store.get(current_hash)?.clone();
        let other_tree = self.store.get(other_hash)?.clone();

        let patch_a = diff_trees(&ancestor_tree, &current_tree);
        let patch_b = diff_trees(&ancestor_tree, &other_tree);

        let merge_result = merge_patches(&patch_a, &patch_b);

        if merge_result.is_clean() {
            // Apply merged patch to ancestor
            let mut result_tree = ancestor_tree;
            apply_patch(&mut result_tree, &merge_result.merged_ops);
            self.commit(
                &result_tree,
                &alloc_format("merge branch '{}'", other_branch),
                "system",
            );
        }

        Some(merge_result)
    }

    /// Get current HEAD hash
    pub fn head_hash(&self) -> Hash {
        self.branches
            .get(&self.current_branch)
            .map(|b| b.head)
            .unwrap_or(0)
    }

    /// Get current HEAD tree
    pub fn head_tree(&self) -> Option<&AstTree> {
        self.store.get(self.head_hash())
    }

    /// Get commit by hash
    pub fn get_commit(&self, hash: Hash) -> Option<&Commit> {
        self.commits.get(&hash)
    }

    /// List branches
    pub fn branch_names(&self) -> Vec<&str> {
        self.branches.keys().map(|s| s.as_str()).collect()
    }

    /// Current branch name
    pub fn current_branch(&self) -> &str {
        &self.current_branch
    }

    /// Total commit count
    pub fn commit_count(&self) -> usize {
        self.commits.len()
    }

    /// Get diff between two commits
    pub fn diff(&self, from: Hash, to: Hash) -> Option<Vec<DiffOp>> {
        let from_tree = self.store.get(from)?;
        let to_tree = self.store.get(to)?;
        Some(diff_trees(from_tree, to_tree))
    }
}

/// Format helper for no_std
fn alloc_format(template: &str, arg: &str) -> String {
    let mut s = String::from(template);
    if let Some(pos) = s.find("'{}'") {
        s.replace_range(pos..pos + 4, arg);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNodeKind, AstTree, NodeValue};

    #[test]
    fn test_repository_init() {
        let repo = Repository::new();
        assert_eq!(repo.commit_count(), 1);
        assert_eq!(repo.current_branch(), "main");
    }

    #[test]
    fn test_commit() {
        let mut repo = Repository::new();
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Primitive, "sphere", 0);

        let hash = repo.commit(&tree, "add sphere", "test");
        assert_eq!(repo.commit_count(), 2);

        let commit = repo.get_commit(hash).unwrap();
        assert_eq!(commit.message, "add sphere");
    }

    #[test]
    fn test_branch_and_checkout() {
        let mut repo = Repository::new();
        repo.create_branch("feature");
        assert!(repo.checkout("feature"));
        assert_eq!(repo.current_branch(), "feature");
        assert!(!repo.checkout("nonexistent"));
    }

    #[test]
    fn test_diff_between_commits() {
        let mut repo = Repository::new();
        let h1 = repo.head_hash();

        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        let h2 = repo.commit(&tree, "add sphere", "test");

        let ops = repo.diff(h1, h2).unwrap();
        assert!(!ops.is_empty());
    }

    #[test]
    fn test_commit_stores_patch() {
        let mut repo = Repository::new();
        let mut tree = AstTree::new();
        let s = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        tree.add_node_with_value(AstNodeKind::Parameter, "radius", NodeValue::Float(1.0), s);

        let hash = repo.commit(&tree, "add sphere", "test");
        let commit = repo.get_commit(hash).unwrap();
        assert!(!commit.patch.is_empty());
    }

    #[test]
    fn test_branch_names() {
        let mut repo = Repository::new();
        repo.create_branch("dev");
        repo.create_branch("feature");
        let names = repo.branch_names();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"dev"));
        assert!(names.contains(&"feature"));
    }

    // ── New tests ──────────────────────────────────────────────────────

    #[test]
    fn test_repository_default_equals_new() {
        let r1 = Repository::new();
        let r2 = Repository::default();
        assert_eq!(r1.commit_count(), r2.commit_count());
        assert_eq!(r1.current_branch(), r2.current_branch());
    }

    #[test]
    fn test_head_hash_changes_after_commit() {
        let mut repo = Repository::new();
        let initial_head = repo.head_hash();
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        repo.commit(&tree, "add sphere", "alice");
        assert_ne!(repo.head_hash(), initial_head);
    }

    #[test]
    fn test_head_tree_reflects_latest_commit() {
        let mut repo = Repository::new();
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Primitive, "sphere", 0);
        repo.commit(&tree, "add sphere", "alice");
        let head = repo.head_tree().unwrap();
        assert_eq!(head.node_count(), tree.node_count());
    }

    #[test]
    fn test_commit_records_author() {
        let mut repo = Repository::new();
        let tree = AstTree::new();
        let hash = repo.commit(&tree, "msg", "bob");
        assert_eq!(repo.get_commit(hash).unwrap().author, "bob");
    }

    #[test]
    fn test_commit_records_message() {
        let mut repo = Repository::new();
        let tree = AstTree::new();
        let hash = repo.commit(&tree, "hello world", "x");
        assert_eq!(repo.get_commit(hash).unwrap().message, "hello world");
    }

    #[test]
    fn test_commit_has_parent() {
        let mut repo = Repository::new();
        let initial_head = repo.head_hash();
        let tree = AstTree::new();
        let hash = repo.commit(&tree, "c2", "x");
        let commit = repo.get_commit(hash).unwrap();
        assert!(commit.parents.contains(&initial_head));
    }

    #[test]
    fn test_checkout_nonexistent_branch_returns_false() {
        let mut repo = Repository::new();
        assert!(!repo.checkout("no-such-branch"));
        assert_eq!(repo.current_branch(), "main"); // unchanged
    }

    #[test]
    fn test_branch_head_advances_after_commit_on_branch() {
        let mut repo = Repository::new();
        repo.create_branch("feat");
        repo.checkout("feat");
        let before = repo.head_hash();
        let mut tree = AstTree::new();
        tree.add_node(AstNodeKind::Group, "g", 0);
        repo.commit(&tree, "on feat", "x");
        assert_ne!(repo.head_hash(), before);
    }

    #[test]
    fn test_diff_between_same_commit_is_empty() {
        let repo = Repository::new();
        let h = repo.head_hash();
        let ops = repo.diff(h, h).unwrap();
        assert!(ops.is_empty());
    }

    #[test]
    fn test_get_commit_nonexistent_returns_none() {
        let repo = Repository::new();
        assert!(repo.get_commit(0xDEAD_BEEF_CAFE_BABE).is_none());
    }
}
