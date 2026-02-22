# ALICE-VCS

**Procedural Version Control — Don't diff lines, diff the AST**

> "A 3D model change is not a text change. It's a tree transformation."

```
Traditional Git:  SDF file edit → binary diff → 50 KB patch
ALICE-VCS:        SDF AST edit  → semantic diff → 12 bytes patch
```

## The Problem

Git diffs text files line-by-line. But ALICE-SDF, ALICE-Animation, and ALICE-Manga produce **tree-structured mathematical data** (CSG trees, scene graphs, panel layouts). A line-based diff of serialized binary is meaningless — moving one node in a scene graph can produce a 100 KB binary diff even though the semantic change is "translate sphere +5 on X axis".

Collaborative creation on procedural data needs **semantic versioning**.

## The Solution

Instead of diffing serialized bytes, diff **the abstract syntax tree (AST)** of the procedural data:

- **Tree Edit Distance** — minimum insertions, deletions, and relabels to transform AST₁ into AST₂
- **Operation-Based Patches** — `Insert(node, parent, index)`, `Delete(node)`, `Update(node, value)`, `Relabel(node, label)`, `Move(node, new_parent, index)`
- **Conflict Resolution** — structural merge on non-overlapping subtrees, manual merge on conflicts

A change like "scale the sphere's radius from 1.0 to 1.5" becomes a single `Update` operation of ~14 bytes, regardless of the serialized file size.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          ALICE-VCS                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────┐     │
│  │  AstTree     │──▶│  Diff Engine │──▶│  Patch Codec     │     │
│  │  (generic    │   │  diff_trees()│   │  LEB128 varint   │     │
│  │   tree)      │   │  O(m+n) match│   │  (4-12 B / op)   │     │
│  └──────────────┘   └──────┬───────┘   └──────────────────┘     │
│                             │                                     │
│              ┌──────────────┼──────────────┐                     │
│              ▼              ▼              ▼                     │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐            │
│  │  Snapshot    │ │  Merge       │ │  History      │            │
│  │  Store       │ │  Engine      │ │  DAG          │            │
│  │  (FNV-1a     │ │  (3-way AST) │ │  (commit log) │            │
│  │   Merkle DAG)│ │              │ │               │            │
│  └──────────────┘ └──────────────┘ └──────────────┘            │
│                                                                   │
│  ┌─────────────────────────────────────────────────┐            │
│  │  GC Engine                                       │            │
│  │  Mark-sweep over snapshot DAG                    │            │
│  │  dry_run() + collect_garbage()                   │            │
│  └─────────────────────────────────────────────────┘            │
│                                                                   │
│  ┌─────────────────────────────────────────────────┐            │
│  │  P2P Sync (Planned)                              │            │
│  │  Push/Pull patches over event diffing            │            │
│  │  Conflict detection via vector clocks            │            │
│  └─────────────────────────────────────────────────┘            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## AST Node Kinds

ALICE-VCS uses a single generic `AstTree` structure. Node kinds cover the common procedural data domains:

| `AstNodeKind` | Repr (`u8`) | Intended Use |
|---------------|-------------|--------------|
| `Root` | 0 | Tree root (always node ID 0) |
| `CsgOp` | 1 | CSG operation: union, subtract, intersect |
| `Primitive` | 2 | Geometric primitive: sphere, box, cylinder |
| `Transform` | 3 | Transform: translate, rotate, scale |
| `Parameter` | 4 | Numeric parameter: radius, width, angle |
| `Group` | 5 | Scene or logical group node |
| `Material` | 6 | Material or shader definition |
| `Keyframe` | 7 | Animation keyframe |
| `Custom` | 255 | Extension / user-defined node |

All values outside 0–7 decode to `Custom`. Domain-specific AST types for Animation Scene Graph, Manga Panel Layout, Synth Score, and others are **Planned** — they will be represented as specialised subtrees using the existing kinds, with dedicated integrations gated behind the `sdf`, `sync`, `db`, and `auth` feature flags once those crates are connected.

## Patch Format

The codec (`src/codec.rs`) uses **LEB128 varint encoding** (unsigned, little-endian base-128). There is no fixed-width header struct and no magic byte sequence.

```
Patch byte stream:
  [varint: op_count]
  for each op:
    [u8: op_type]  0=Insert 1=Delete 2=Update 3=Relabel 4=Move
    ... op-specific fields (varints + value payload) ...
```

### Op encoding

| Op | Fields | Typical size (small IDs) |
|----|--------|--------------------------|
| `Delete` | op_type(1) + node_id(varint) | 2 bytes |
| `Move` | op_type(1) + node_id + new_parent_id + new_index (varints) | 4 bytes |
| `Update` | op_type(1) + node_id(varint) + old_value + new_value | 5 + 2×value bytes |
| `Relabel` | op_type(1) + node_id(varint) + old_label(len+bytes) + new_label | variable |
| `Insert` | op_type(1) + parent_id + index + kind(1) + label + value | variable |

### Value encoding

| `NodeValue` variant | Tag byte | Payload |
|---------------------|----------|---------|
| `None` | 0x00 | — |
| `Int(i64)` | 0x01 | 8 bytes LE |
| `Float(f64)` | 0x02 | 8 bytes LE |
| `Text(String)` | 0x03 | varint(len) + UTF-8 bytes |
| `Ident(String)` | 0x04 | varint(len) + UTF-8 bytes |
| `Bytes(Vec<u8>)` | 0x05 | varint(len) + raw bytes |

### Size Comparison

| Change | Git Binary Diff | ALICE-VCS Patch | Ratio |
|--------|----------------|----------------|-------|
| Change sphere radius | ~2 KB | **~14 bytes** | **~140x** |
| Move node in scene | ~50 KB | **~4 bytes** | **~12,500x** |
| Add CSG subtraction | ~5 KB | **~20 bytes** | **~250x** |
| Full episode edit (50 changes) | ~500 KB | **~600 bytes** | **~833x** |

## Diff Engine

`diff_trees(old, new)` returns a `Vec<DiffOp>` representing the minimal edit to transform `old` into `new`.

### O(1) HashMap Child Matching

The diff engine uses a `HashMap<(AstNodeKind, label), Vec<index>>` to match children between two nodes in O(m+n) time instead of the naive O(m×n) nested-loop approach. For each level of the tree:

1. Build a `HashMap` from `(kind, label)` to candidate indices in the new child list.
2. For each old child, look up its `(kind, label)` key in O(1) and claim the first unmatched candidate.
3. Unmatched old children become `Delete` ops; unmatched new children become `Insert` ops.

This means a flat node with 1,000 children is diffed in O(1,000) rather than O(1,000,000).

The same pattern appears in `AstTree::remove_subtree` — removed node IDs are placed in a `HashMap<NodeId, ()>` so that the `retain()` membership check is O(1) per surviving node instead of O(n).

## Merge Strategy

### 3-Way Structural Merge

```
           Base (AST₀)
          /            \
    Ours (AST₁)    Theirs (AST₂)
          \            /
         Merged (AST₃)
```

`merge_patches(patch_a, patch_b)` operates on two `Vec<DiffOp>` computed from a common ancestor:

| Scenario | Resolution |
|----------|-----------|
| Non-overlapping subtrees | Auto-merge (no conflict) |
| Same node, same operation in both patches | Auto-resolve (deduplicated) |
| Same node, different operations | **Conflict** — manual resolve |
| Delete vs. modify same node | **Conflict** — manual resolve |

Conflict detection uses a `HashSet<NodeId>` built from each patch, giving O(1) membership tests when classifying each operation as conflicting or clean.

## API

```rust
use alice_vcs::{
    Repository, AstTree, AstNodeKind, NodeValue,
    diff_trees, apply_patch, patch_size_bytes,
    encode_patch, decode_patch,
    merge_patches, MergeResult,
    collect_garbage, dry_run,
};

// Initialize an in-memory repository
let mut repo = Repository::new();

// Build an AST
let mut tree = AstTree::new();
let sphere = tree.add_node(AstNodeKind::Primitive, "sphere", 0);
tree.add_node_with_value(AstNodeKind::Parameter, "radius", NodeValue::Float(1.0), sphere);

// Commit a snapshot
let h1 = repo.commit(&tree, "add sphere", "author");

// Modify and compute semantic diff
let mut tree2 = tree.clone();
tree2.get_node_mut(2).unwrap().value = NodeValue::Float(1.5);

let ops = diff_trees(&tree, &tree2);
// ops = [Update { node_id: 2, old_value: Float(1.0), new_value: Float(1.5) }]

// Commit the change
let h2 = repo.commit(&tree2, "scale radius to 1.5", "author");

// Encode patch to bytes (LEB128 varint)
let bytes = encode_patch(&ops);

// Decode patch from bytes
let decoded_ops = decode_patch(&bytes).unwrap();

// Inspect diff between two commits
let diff_ops = repo.diff(h1, h2).unwrap();

// Branching
repo.create_branch("feature");
repo.checkout("feature");
// ... make changes and commit on "feature" ...
repo.checkout("main");

// 3-way structural merge
let merge_result = repo.merge("feature");
match merge_result {
    Some(result) if result.is_clean() => { /* auto-merged */ }
    Some(result) => {
        for conflict in &result.conflicts {
            // conflict.node_id, conflict.ops_a, conflict.ops_b
        }
    }
    None => { /* branch not found */ }
}

// Garbage collection — removes unreachable snapshots from the store
let head = repo.head_hash();
// (SnapshotStore is internal to Repository; GC is exposed for
//  external store users via collect_garbage / dry_run)
```

## Modules

| Module | File | Exports |
|--------|------|---------|
| `ast` | `src/ast.rs` | `AstTree`, `AstNode`, `AstNodeKind`, `NodeId`, `NodeValue` |
| `diff` | `src/diff.rs` | `diff_trees()`, `apply_patch()`, `patch_size_bytes()`, `DiffOp` |
| `codec` | `src/codec.rs` | `encode_patch()`, `decode_patch()`, `encoded_patch_size()` |
| `commit` | `src/commit.rs` | `Repository`, `Commit`, `Branch` |
| `merge` | `src/merge.rs` | `merge_patches()`, `MergeResult`, `Conflict` |
| `store` | `src/store.rs` | `SnapshotStore`, `Hash` |
| `gc` | `src/gc.rs` | `collect_garbage()`, `dry_run()`, `GcResult` |

## Ecosystem Integration (Planned)

```
ALICE-SDF ──── AST ────▶ ALICE-VCS ──── Patches ────▶ ALICE-Sync (Planned)
ALICE-Animation ─ AST ─▶     │                             │
ALICE-Manga ──── AST ──▶     │                             ▼
                              ▼                        ALICE-DB (Planned)
                         ALICE-Auth (Planned)
                      (commit signing)
```

| Bridge | Direction | Status |
|--------|-----------|--------|
| SDF → VCS | Parse CSG tree into diffable AstTree | Planned |
| Animation → VCS | Parse SceneGraph into diffable AstTree | Planned |
| VCS → Sync | Distribute patches via P2P event diffing | Planned |
| VCS → DB | Persist snapshots and commit history | Planned |
| VCS → Auth | Ed25519 commit signing and verification | Planned |

## Feature Flags (Planned)

The feature flags below are declared in `Cargo.toml`. Their dependency crates are commented out and not yet integrated — enabling these flags currently has no effect beyond pulling in `std`.

| Feature | Would depend on | Description |
|---------|----------------|-------------|
| *(default)* | None | Core diff engine, `no_std` compatible |
| `std` | std | Enables `HashMap`/`HashSet` (vs `BTreeMap`/`BTreeSet` in `no_std`) |
| `sdf` (Planned) | alice-sdf | ALICE-SDF CSG tree diffing |
| `sync` (Planned) | alice-sync | P2P patch replication |
| `db` (Planned) | alice-db | Snapshot persistence |
| `auth` (Planned) | alice-auth | Commit signing (Ed25519) |

## Tests

149 unit tests across 7 modules (ast: 32, codec: 30, diff: 32, commit: 16, merge: 12, gc: 16, store: 11).

```bash
cargo test
```

## License

AGPL-3.0

## Author

Moroya Sakamoto

---

*"Version control should understand structure, not just text."*
