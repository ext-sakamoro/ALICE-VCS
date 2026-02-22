# Contributing to ALICE-VCS

## Build

```bash
cargo build
cargo build --features std
```

## Test

```bash
cargo test --features std
```

> Default features are empty (`no_std` + `alloc`). Use `--features std` to run the full test suite.

## Lint

```bash
cargo clippy --features std -- -W clippy::all
cargo fmt -- --check
cargo doc --features std --no-deps 2>&1 | grep warning
```

## Design Constraints

- **Don't diff lines, diff the AST**: all version control operates on tree-structured procedural data, not text.
- **Compact patches**: each DiffOp encodes in 4-12 bytes, replacing 50 KB binary diffs.
- **Content-addressed storage**: snapshots stored in a Merkle DAG with FNV-1a hashing.
- **Structural merge**: 3-way merge detects conflicts at the AST node level.
- **`no_std` core**: runs on embedded/WASM with `alloc`; `std` is opt-in.
- **Zero external dependencies**: all AST, diff, merge, codec, and store logic is self-contained.
