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
