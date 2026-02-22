//! ALICE-VCS — Procedural Version Control
//!
//! Don't diff lines, diff the AST.
//!
//! Semantic diff and merge for tree-structured procedural data:
//! - AST node-level diff (12-byte patches vs 50 KB binary diffs)
//! - Structural 3-way merge with conflict detection
//! - Content-addressed snapshot store (Merkle DAG)
//! - Commit/branch model for procedural collaboration
//!
//! # Modules
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`ast`] | Generic AST tree with node kinds, values, and O(1) lookup |
//! | [`codec`] | Binary patch encoding/decoding (4-12 bytes per op) |
//! | [`commit`] | Commit, branch, and repository model |
//! | [`diff`] | Minimal AST diff engine (Insert, Delete, Update, Move, Relabel) |
//! | [`gc`] | Garbage collection for unreachable snapshots |
//! | [`merge`] | Structural 3-way merge with conflict detection |
//! | [`store`] | Content-addressed Merkle DAG snapshot store |
//!
//! # Feature flags
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `std` | Enable `std` collections (default: `no_std` + `alloc`) |
//! | `sdf` | ALICE-SDF AST diff (future) |
//! | `sync` | ALICE-Sync P2P replication (future) |
//! | `db` | ALICE-DB snapshot storage (future) |
//! | `auth` | ALICE-Auth commit signing (future) |
//!
//! # Quick Start
//!
//! ```
//! use alice_vcs::{AstTree, AstNodeKind, NodeValue, diff_trees};
//!
//! // Build a tree with a sphere primitive
//! let mut old = AstTree::new();
//! let root = old.root_id();
//! let _sphere = old.add_node_with_value(
//!     AstNodeKind::Primitive, "sphere", NodeValue::Float(1.0), root,
//! );
//!
//! // Clone and change the sphere's radius
//! let mut new = old.clone();
//! new.get_node_mut(_sphere).unwrap().value = NodeValue::Float(2.0);
//!
//! // Diff produces a single Update op
//! let ops = diff_trees(&old, &new);
//! assert_eq!(ops.len(), 1);
//! ```
//!
//! Author: Moroya Sakamoto

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod ast;
pub mod codec;
pub mod commit;
pub mod diff;
pub mod gc;
pub mod merge;
pub mod store;

pub use ast::{AstNode, AstNodeKind, AstTree, NodeId, NodeValue};
pub use codec::{decode_patch, encode_patch, encoded_patch_size};
pub use commit::{Branch, Commit, Repository};
pub use diff::{diff_trees, DiffOp};
pub use gc::{collect_garbage, dry_run, GcResult};
pub use merge::{merge_patches, Conflict, MergeResult};
pub use store::{Hash, SnapshotStore};
