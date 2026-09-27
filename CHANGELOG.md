# Changelog

All notable changes to ALICE-VCS will be documented in this file.

## [Unreleased]

### Changed
- **License: `AGPL-3.0` → `AGPL-3.0 OR LicenseRef-Commercial` (dual-licensed、2026-09-27)** AGPL 側の条件は変更なし (既存 AGPL 利用者への影響ゼロ)、商用という選択肢が追加されただけ SPDX が AGPL 単独だと cargo-deny / FOSSA / SBOM に「商用オプションなし」と見えるため宣言を dual に 変更点: SPDX / `LICENSE` → `LICENSE-AGPL` / `LICENSE-COMMERCIAL.md` (商用トリガー 6 条件 = クローズド製品・商用 SaaS・エッジ / ファームウェア配布・plugin 再配布・プラットフォーム NDA・保証、社内利用は AGPL 側で無償と明記) / README の選択肢表 商用窓口は法人 `contact@extoria.co.jp`

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
