//! C-ABI FFI bindings for ALICE-VCS
//!
//! 20 `extern "C"` functions for AST tree, diff, commit, and repository.
//!
//! Author: Moroya Sakamoto

use crate::ast::{AstNodeKind, AstTree, NodeValue};
use crate::commit::Repository;
use crate::diff::{diff_trees, patch_size_bytes};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// ============================================================================
// Opaque handles
// ============================================================================

/// Opaque handle to AstTree
pub type AliceAstTreeHandle = *mut AstTree;

/// Opaque handle to Repository
pub type AliceRepoHandle = *mut Repository;

// ============================================================================
// C-compatible structs
// ============================================================================

/// Diff statistics
#[repr(C)]
pub struct AliceVcsDiffStats {
    pub insert_count: u32,
    pub delete_count: u32,
    pub update_count: u32,
    pub relabel_count: u32,
    pub move_count: u32,
    pub total_ops: u32,
    pub patch_bytes: u32,
}

// ============================================================================
// AstTree lifecycle
// ============================================================================

/// Create a new empty AST tree.
#[no_mangle]
pub extern "C" fn alice_vcs_tree_create() -> AliceAstTreeHandle {
    Box::into_raw(Box::new(AstTree::new()))
}

/// Destroy an AST tree.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `alice_vcs_tree_create`.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_destroy(handle: AliceAstTreeHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

// ============================================================================
// AstTree operations
// ============================================================================

/// Add a node to the tree. Returns the new node ID.
///
/// # Safety
///
/// `handle` must be valid. `label` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_add_node(
    handle: AliceAstTreeHandle,
    kind: u8,
    label: *const c_char,
    parent_id: u32,
) -> u32 {
    if handle.is_null() || label.is_null() {
        return u32::MAX;
    }
    let tree = unsafe { &mut *handle };
    let label_str = match unsafe { CStr::from_ptr(label) }.to_str() {
        Ok(s) => s,
        Err(_) => return u32::MAX,
    };
    tree.add_node(AstNodeKind::from_u8(kind), label_str, parent_id)
}

/// Add a node with a float value. Returns the new node ID.
///
/// # Safety
///
/// `handle` must be valid. `label` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_add_node_float(
    handle: AliceAstTreeHandle,
    kind: u8,
    label: *const c_char,
    value: f64,
    parent_id: u32,
) -> u32 {
    if handle.is_null() || label.is_null() {
        return u32::MAX;
    }
    let tree = unsafe { &mut *handle };
    let label_str = match unsafe { CStr::from_ptr(label) }.to_str() {
        Ok(s) => s,
        Err(_) => return u32::MAX,
    };
    tree.add_node_with_value(
        AstNodeKind::from_u8(kind),
        label_str,
        NodeValue::Float(value),
        parent_id,
    )
}

/// Get node count.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_node_count(handle: AliceAstTreeHandle) -> u32 {
    if handle.is_null() {
        return 0;
    }
    let tree = unsafe { &*handle };
    tree.node_count() as u32
}

/// Get the root node ID.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_root_id(handle: AliceAstTreeHandle) -> u32 {
    if handle.is_null() {
        return u32::MAX;
    }
    let tree = unsafe { &*handle };
    tree.root_id()
}

/// Get node label. Caller must free with `alice_vcs_string_free`.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_get_label(
    handle: AliceAstTreeHandle,
    node_id: u32,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let tree = unsafe { &*handle };
    match tree.get_node(node_id) {
        Some(node) => match CString::new(node.label.clone()) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

/// Get node kind as u8.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_get_kind(handle: AliceAstTreeHandle, node_id: u32) -> u8 {
    if handle.is_null() {
        return 255;
    }
    let tree = unsafe { &*handle };
    tree.get_node(node_id).map_or(255, |n| n.kind as u8)
}

/// Compute subtree hash (FNV-1a).
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_subtree_hash(
    handle: AliceAstTreeHandle,
    node_id: u32,
) -> u64 {
    if handle.is_null() {
        return 0;
    }
    let tree = unsafe { &*handle };
    tree.subtree_hash(node_id)
}

/// Remove a subtree.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_tree_remove_subtree(handle: AliceAstTreeHandle, node_id: u32) {
    if handle.is_null() {
        return;
    }
    let tree = unsafe { &mut *handle };
    tree.remove_subtree(node_id);
}

// ============================================================================
// Diff
// ============================================================================

/// Diff two trees. Returns diff statistics.
///
/// # Safety
///
/// Both handles must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_diff(
    old: AliceAstTreeHandle,
    new: AliceAstTreeHandle,
    out: *mut AliceVcsDiffStats,
) -> u8 {
    if old.is_null() || new.is_null() || out.is_null() {
        return 0;
    }
    let old_tree = unsafe { &*old };
    let new_tree = unsafe { &*new };
    let ops = diff_trees(old_tree, new_tree);
    use crate::diff::DiffOp;
    unsafe {
        (*out).insert_count = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Insert { .. }))
            .count() as u32;
        (*out).delete_count = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Delete { .. }))
            .count() as u32;
        (*out).update_count = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Update { .. }))
            .count() as u32;
        (*out).relabel_count = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Relabel { .. }))
            .count() as u32;
        (*out).move_count = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Move { .. }))
            .count() as u32;
        (*out).total_ops = ops.len() as u32;
        (*out).patch_bytes = patch_size_bytes(&ops) as u32;
    }
    1
}

// ============================================================================
// Repository
// ============================================================================

/// Create a new repository.
#[no_mangle]
pub extern "C" fn alice_vcs_repo_create() -> AliceRepoHandle {
    Box::into_raw(Box::new(Repository::new()))
}

/// Destroy a repository.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_repo_destroy(handle: AliceRepoHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Commit a tree to the repository. Returns the commit hash.
///
/// # Safety
///
/// Both handles must be valid. `message`/`author` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_repo_commit(
    handle: AliceRepoHandle,
    tree: AliceAstTreeHandle,
    message: *const c_char,
    author: *const c_char,
) -> u64 {
    if handle.is_null() || tree.is_null() || message.is_null() || author.is_null() {
        return 0;
    }
    let repo = unsafe { &mut *handle };
    let tree_ref = unsafe { &*tree };
    let msg = match unsafe { CStr::from_ptr(message) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let auth = match unsafe { CStr::from_ptr(author) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    repo.commit(tree_ref, msg, auth)
}

/// Get current HEAD hash.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_repo_head_hash(handle: AliceRepoHandle) -> u64 {
    if handle.is_null() {
        return 0;
    }
    let repo = unsafe { &*handle };
    repo.head_hash()
}

/// Get commit count.
///
/// # Safety
///
/// `handle` must be valid.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_repo_commit_count(handle: AliceRepoHandle) -> u32 {
    if handle.is_null() {
        return 0;
    }
    let repo = unsafe { &*handle };
    repo.commit_count() as u32
}

/// Create a branch.
///
/// # Safety
///
/// `handle` must be valid. `name` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_repo_create_branch(
    handle: AliceRepoHandle,
    name: *const c_char,
) -> u8 {
    if handle.is_null() || name.is_null() {
        return 0;
    }
    let repo = unsafe { &mut *handle };
    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    repo.create_branch(name_str);
    1
}

/// Checkout a branch.
///
/// # Safety
///
/// `handle` must be valid. `name` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_repo_checkout(
    handle: AliceRepoHandle,
    name: *const c_char,
) -> u8 {
    if handle.is_null() || name.is_null() {
        return 0;
    }
    let repo = unsafe { &mut *handle };
    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    repo.checkout(name_str) as u8
}

// ============================================================================
// Memory management
// ============================================================================

/// Free a string returned by FFI functions.
///
/// # Safety
///
/// `s` must be a pointer returned by an alice_vcs FFI function.
#[no_mangle]
pub unsafe extern "C" fn alice_vcs_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

// ============================================================================
// Version
// ============================================================================

/// Get library version string.
#[no_mangle]
pub extern "C" fn alice_vcs_version() -> *const c_char {
    c"0.1.0".as_ptr()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_tree_create_destroy() {
        let handle = alice_vcs_tree_create();
        assert!(!handle.is_null());
        unsafe { alice_vcs_tree_destroy(handle) };
    }

    #[test]
    fn test_tree_add_node_and_count() {
        let handle = alice_vcs_tree_create();
        let label = CString::new("sphere").unwrap();
        let id = unsafe { alice_vcs_tree_add_node(handle, 2, label.as_ptr(), 0) };
        assert_ne!(id, u32::MAX);
        assert_eq!(unsafe { alice_vcs_tree_node_count(handle) }, 2);
        unsafe { alice_vcs_tree_destroy(handle) };
    }

    #[test]
    fn test_tree_add_node_float() {
        let handle = alice_vcs_tree_create();
        let label = CString::new("radius").unwrap();
        let id = unsafe { alice_vcs_tree_add_node_float(handle, 4, label.as_ptr(), 3.14, 0) };
        assert_ne!(id, u32::MAX);
        unsafe { alice_vcs_tree_destroy(handle) };
    }

    #[test]
    fn test_tree_root_id() {
        let handle = alice_vcs_tree_create();
        assert_eq!(unsafe { alice_vcs_tree_root_id(handle) }, 0);
        unsafe { alice_vcs_tree_destroy(handle) };
    }

    #[test]
    fn test_tree_get_label() {
        let handle = alice_vcs_tree_create();
        let label = CString::new("box").unwrap();
        let id = unsafe { alice_vcs_tree_add_node(handle, 2, label.as_ptr(), 0) };
        let got = unsafe { alice_vcs_tree_get_label(handle, id) };
        assert!(!got.is_null());
        let result = unsafe { CStr::from_ptr(got) }.to_str().unwrap();
        assert_eq!(result, "box");
        unsafe {
            alice_vcs_string_free(got);
            alice_vcs_tree_destroy(handle);
        }
    }

    #[test]
    fn test_tree_get_kind() {
        let handle = alice_vcs_tree_create();
        let label = CString::new("s").unwrap();
        unsafe { alice_vcs_tree_add_node(handle, 2, label.as_ptr(), 0) };
        let kind = unsafe { alice_vcs_tree_get_kind(handle, 1) };
        assert_eq!(kind, 2); // Primitive
        unsafe { alice_vcs_tree_destroy(handle) };
    }

    #[test]
    fn test_tree_subtree_hash() {
        let handle = alice_vcs_tree_create();
        let label = CString::new("sphere").unwrap();
        unsafe { alice_vcs_tree_add_node(handle, 2, label.as_ptr(), 0) };
        let hash = unsafe { alice_vcs_tree_subtree_hash(handle, 0) };
        assert_ne!(hash, 0);
        unsafe { alice_vcs_tree_destroy(handle) };
    }

    #[test]
    fn test_tree_remove_subtree() {
        let handle = alice_vcs_tree_create();
        let label = CString::new("temp").unwrap();
        let id = unsafe { alice_vcs_tree_add_node(handle, 5, label.as_ptr(), 0) };
        assert_eq!(unsafe { alice_vcs_tree_node_count(handle) }, 2);
        unsafe { alice_vcs_tree_remove_subtree(handle, id) };
        assert_eq!(unsafe { alice_vcs_tree_node_count(handle) }, 1);
        unsafe { alice_vcs_tree_destroy(handle) };
    }

    #[test]
    fn test_diff_identical() {
        let t1 = alice_vcs_tree_create();
        let t2 = alice_vcs_tree_create();
        let label = CString::new("sphere").unwrap();
        unsafe {
            alice_vcs_tree_add_node(t1, 2, label.as_ptr(), 0);
            alice_vcs_tree_add_node(t2, 2, label.as_ptr(), 0);
        }
        let mut stats = AliceVcsDiffStats {
            insert_count: 0,
            delete_count: 0,
            update_count: 0,
            relabel_count: 0,
            move_count: 0,
            total_ops: 0,
            patch_bytes: 0,
        };
        let ok = unsafe { alice_vcs_diff(t1, t2, &mut stats) };
        assert_eq!(ok, 1);
        assert_eq!(stats.total_ops, 0);
        unsafe {
            alice_vcs_tree_destroy(t1);
            alice_vcs_tree_destroy(t2);
        }
    }

    #[test]
    fn test_diff_with_insert() {
        let t1 = alice_vcs_tree_create();
        let t2 = alice_vcs_tree_create();
        let label = CString::new("sphere").unwrap();
        unsafe { alice_vcs_tree_add_node(t2, 2, label.as_ptr(), 0) };
        let mut stats = AliceVcsDiffStats {
            insert_count: 0,
            delete_count: 0,
            update_count: 0,
            relabel_count: 0,
            move_count: 0,
            total_ops: 0,
            patch_bytes: 0,
        };
        unsafe { alice_vcs_diff(t1, t2, &mut stats) };
        assert_eq!(stats.insert_count, 1);
        unsafe {
            alice_vcs_tree_destroy(t1);
            alice_vcs_tree_destroy(t2);
        }
    }

    #[test]
    fn test_repo_create_destroy() {
        let handle = alice_vcs_repo_create();
        assert!(!handle.is_null());
        assert!(unsafe { alice_vcs_repo_commit_count(handle) } >= 1);
        unsafe { alice_vcs_repo_destroy(handle) };
    }

    #[test]
    fn test_repo_commit_and_head() {
        let repo = alice_vcs_repo_create();
        let tree = alice_vcs_tree_create();
        let label = CString::new("sphere").unwrap();
        unsafe { alice_vcs_tree_add_node(tree, 2, label.as_ptr(), 0) };
        let msg = CString::new("add sphere").unwrap();
        let author = CString::new("test").unwrap();
        let hash = unsafe { alice_vcs_repo_commit(repo, tree, msg.as_ptr(), author.as_ptr()) };
        assert_ne!(hash, 0);
        let head = unsafe { alice_vcs_repo_head_hash(repo) };
        assert_eq!(head, hash);
        unsafe {
            alice_vcs_tree_destroy(tree);
            alice_vcs_repo_destroy(repo);
        }
    }

    #[test]
    fn test_repo_branch() {
        let repo = alice_vcs_repo_create();
        let name = CString::new("feature").unwrap();
        let ok = unsafe { alice_vcs_repo_create_branch(repo, name.as_ptr()) };
        assert_eq!(ok, 1);
        let co = unsafe { alice_vcs_repo_checkout(repo, name.as_ptr()) };
        assert_eq!(co, 1);
        unsafe { alice_vcs_repo_destroy(repo) };
    }

    #[test]
    fn test_null_safety() {
        unsafe {
            alice_vcs_tree_destroy(std::ptr::null_mut());
            alice_vcs_repo_destroy(std::ptr::null_mut());
            alice_vcs_string_free(std::ptr::null_mut());
            assert_eq!(alice_vcs_tree_node_count(std::ptr::null_mut()), 0);
            assert_eq!(alice_vcs_repo_head_hash(std::ptr::null_mut()), 0);
        }
    }

    #[test]
    fn test_version() {
        let v = alice_vcs_version();
        let version = unsafe { CStr::from_ptr(v) }.to_str().unwrap();
        assert_eq!(version, "0.1.0");
    }
}
