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
pub mod diff;
pub mod merge;
pub mod store;
pub mod commit;

pub use ast::{AstNode, AstNodeKind, NodeId};
pub use diff::{DiffOp, diff_trees};
pub use merge::{merge_patches, MergeResult, Conflict};
pub use store::{SnapshotStore, Hash};
pub use commit::{Commit, Branch, Repository};
