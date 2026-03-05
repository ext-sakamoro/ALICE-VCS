# Changelog

All notable changes to ALICE-VCS will be documented in this file.

## [0.1.1] - 2026-03-04

### Added
- `ffi` — 20 `extern "C"` FFI functions (AstTree, diff, Repository, branch)
- Unity C# bindings (`bindings/unity/AliceVcs.cs`) — 20 DllImport + AstTree/Repository classes
- UE5 C++ header (`bindings/ue5/AliceVcs.h`) — 20 extern C + RAII FAstTree/FRepository wrappers

### Fixed
- `cargo fmt` trailing whitespace in source files

## [0.1.0] - 2026-02-23

### Added
- `ast` — `AstTree`, `AstNode`, `AstNodeKind` (Root/CsgOp/Primitive/Transform/Parameter/Group/Material/Keyframe/Custom), `NodeValue`, O(1) HashMap index
- `diff` — `diff_trees` minimal edit script: Insert, Delete, Update, Relabel, Move
- `codec` — binary patch encoding/decoding (4-12 bytes per op)
- `merge` — structural 3-way `merge_patches` with `Conflict` detection
- `store` — content-addressed `SnapshotStore` (Merkle DAG, FNV-1a hashing)
- `commit` — `Commit`, `Branch`, `Repository` model
- `gc` — `collect_garbage` / `dry_run` for unreachable snapshot removal
- `no_std` + `alloc` support (`std` feature opt-in)
- 149 unit tests + 1 doc-test

### Fixed
- `or_insert_with(Vec::new)` → `or_default()` (clippy)
