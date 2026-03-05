// ALICE-VCS UE5 C++ Header
// 20 FFI functions for AST tree, diff, commit, and repository
//
// Author: Moroya Sakamoto

#pragma once

#include <cstdint>
#include <utility>

// ============================================================================
// C API
// ============================================================================

extern "C" {

// Opaque handles
typedef void* AliceAstTreeHandle;
typedef void* AliceRepoHandle;

// Diff statistics
struct AliceVcsDiffStats {
    uint32_t insert_count;
    uint32_t delete_count;
    uint32_t update_count;
    uint32_t relabel_count;
    uint32_t move_count;
    uint32_t total_ops;
    uint32_t patch_bytes;
};

// --- AstTree ---
AliceAstTreeHandle alice_vcs_tree_create();
void     alice_vcs_tree_destroy(AliceAstTreeHandle handle);
uint32_t alice_vcs_tree_add_node(AliceAstTreeHandle handle, uint8_t kind, const char* label, uint32_t parent_id);
uint32_t alice_vcs_tree_add_node_float(AliceAstTreeHandle handle, uint8_t kind, const char* label, double value, uint32_t parent_id);
uint32_t alice_vcs_tree_node_count(AliceAstTreeHandle handle);
uint32_t alice_vcs_tree_root_id(AliceAstTreeHandle handle);
char*    alice_vcs_tree_get_label(AliceAstTreeHandle handle, uint32_t node_id);
uint8_t  alice_vcs_tree_get_kind(AliceAstTreeHandle handle, uint32_t node_id);
uint64_t alice_vcs_tree_subtree_hash(AliceAstTreeHandle handle, uint32_t node_id);
void     alice_vcs_tree_remove_subtree(AliceAstTreeHandle handle, uint32_t node_id);

// --- Diff ---
uint8_t  alice_vcs_diff(AliceAstTreeHandle old_tree, AliceAstTreeHandle new_tree, AliceVcsDiffStats* out);

// --- Repository ---
AliceRepoHandle alice_vcs_repo_create();
void     alice_vcs_repo_destroy(AliceRepoHandle handle);
uint64_t alice_vcs_repo_commit(AliceRepoHandle handle, AliceAstTreeHandle tree, const char* message, const char* author);
uint64_t alice_vcs_repo_head_hash(AliceRepoHandle handle);
uint32_t alice_vcs_repo_commit_count(AliceRepoHandle handle);
uint8_t  alice_vcs_repo_create_branch(AliceRepoHandle handle, const char* name);
uint8_t  alice_vcs_repo_checkout(AliceRepoHandle handle, const char* name);

// --- Memory ---
void alice_vcs_string_free(char* s);

// --- Version ---
const char* alice_vcs_version();

} // extern "C"

// ============================================================================
// RAII C++ Wrapper
// ============================================================================

namespace AliceVcs {

/// AST node kind
enum class ENodeKind : uint8_t {
    Root = 0, CsgOp = 1, Primitive = 2, Transform = 3,
    Parameter = 4, Group = 5, Material = 6, Keyframe = 7, Custom = 255
};

/// RAII wrapper for the AST tree
class FAstTree {
public:
    FAstTree() : Handle(alice_vcs_tree_create()) {}
    ~FAstTree() { if (Handle) alice_vcs_tree_destroy(Handle); }

    FAstTree(FAstTree&& O) noexcept : Handle(O.Handle) { O.Handle = nullptr; }
    FAstTree& operator=(FAstTree&& O) noexcept {
        if (this != &O) { if (Handle) alice_vcs_tree_destroy(Handle); Handle = O.Handle; O.Handle = nullptr; }
        return *this;
    }
    FAstTree(const FAstTree&) = delete;
    FAstTree& operator=(const FAstTree&) = delete;

    uint32_t AddNode(ENodeKind Kind, const char* Label, uint32_t ParentId) {
        return alice_vcs_tree_add_node(Handle, static_cast<uint8_t>(Kind), Label, ParentId);
    }
    uint32_t AddNodeFloat(ENodeKind Kind, const char* Label, double Value, uint32_t ParentId) {
        return alice_vcs_tree_add_node_float(Handle, static_cast<uint8_t>(Kind), Label, Value, ParentId);
    }
    uint32_t NodeCount() const { return alice_vcs_tree_node_count(Handle); }
    uint32_t RootId() const { return alice_vcs_tree_root_id(Handle); }
    uint8_t GetKind(uint32_t NodeId) const { return alice_vcs_tree_get_kind(Handle, NodeId); }
    uint64_t SubtreeHash(uint32_t NodeId) const { return alice_vcs_tree_subtree_hash(Handle, NodeId); }
    void RemoveSubtree(uint32_t NodeId) { alice_vcs_tree_remove_subtree(Handle, NodeId); }

    /// Get label. Caller must free with FreeString().
    char* GetLabel(uint32_t NodeId) const { return alice_vcs_tree_get_label(Handle, NodeId); }

    static void FreeString(char* S) { if (S) alice_vcs_string_free(S); }

    AliceAstTreeHandle GetHandle() const { return Handle; }
    bool IsValid() const { return Handle != nullptr; }

private:
    AliceAstTreeHandle Handle = nullptr;
};

/// Diff two trees
inline bool DiffTrees(const FAstTree& Old, const FAstTree& New, AliceVcsDiffStats& Out) {
    return alice_vcs_diff(Old.GetHandle(), New.GetHandle(), &Out) != 0;
}

/// RAII wrapper for the repository
class FRepository {
public:
    FRepository() : Handle(alice_vcs_repo_create()) {}
    ~FRepository() { if (Handle) alice_vcs_repo_destroy(Handle); }

    FRepository(FRepository&& O) noexcept : Handle(O.Handle) { O.Handle = nullptr; }
    FRepository& operator=(FRepository&& O) noexcept {
        if (this != &O) { if (Handle) alice_vcs_repo_destroy(Handle); Handle = O.Handle; O.Handle = nullptr; }
        return *this;
    }
    FRepository(const FRepository&) = delete;
    FRepository& operator=(const FRepository&) = delete;

    uint64_t Commit(const FAstTree& Tree, const char* Msg, const char* Author) {
        return alice_vcs_repo_commit(Handle, Tree.GetHandle(), Msg, Author);
    }
    uint64_t HeadHash() const { return alice_vcs_repo_head_hash(Handle); }
    uint32_t CommitCount() const { return alice_vcs_repo_commit_count(Handle); }
    bool CreateBranch(const char* Name) { return alice_vcs_repo_create_branch(Handle, Name) != 0; }
    bool Checkout(const char* Name) { return alice_vcs_repo_checkout(Handle, Name) != 0; }

    bool IsValid() const { return Handle != nullptr; }

private:
    AliceRepoHandle Handle = nullptr;
};

} // namespace AliceVcs
