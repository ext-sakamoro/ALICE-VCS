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
- **Operation-Based Patches** — `Insert(node, parent, index)`, `Delete(node)`, `Update(node, field, value)`
- **Conflict Resolution** — structural merge on non-overlapping subtrees, manual merge on conflicts

A change like "scale the sphere's radius from 1.0 to 1.5" becomes a single `Update` operation of 12 bytes, regardless of the serialized file size.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          ALICE-VCS                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────┐     │
│  │  AST Parser  │──▶│  Diff Engine │──▶│  Patch Serializer│     │
│  │  SDF / Scene │   │  Tree Edit   │   │  (4-12 B / op)   │     │
│  │  / Panel     │   │  Distance    │   │                   │     │
│  └──────────────┘   └──────┬───────┘   └──────────────────┘     │
│                             │                                     │
│              ┌──────────────┼──────────────┐                     │
│              ▼              ▼              ▼                     │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐            │
│  │  Snapshot    │ │  Merge       │ │  History      │            │
│  │  Store       │ │  Engine      │ │  DAG          │            │
│  │  (ALICE-DB)  │ │  (3-way AST) │ │  (commit log) │            │
│  └──────────────┘ └──────────────┘ └──────────────┘            │
│                                                                   │
│  ┌─────────────────────────────────────────────────┐            │
│  │  P2P Sync (ALICE-Sync)                           │            │
│  │  Push/Pull patches over event diffing            │            │
│  │  Conflict detection via vector clocks            │            │
│  └─────────────────────────────────────────────────┘            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Supported AST Types

| Data Type | Source Crate | AST Nodes | Example Operations |
|-----------|-------------|-----------|-------------------|
| **SDF CSG Tree** | ALICE-SDF | Union, Subtract, Intersect, Primitive, Transform | Move sphere, change radius, add subtraction |
| **Scene Graph** | ALICE-Animation | Actor, Camera, Timeline, Keyframe, Cut | Re-pose character, adjust camera, edit timing |
| **Panel Layout** | ALICE-Manga | Page, Panel, Balloon, Stroke, Tone | Resize panel, reflow balloon, add screentone |
| **Score** | ALICE-Synth | Track, NoteEvent, Patch, Effect | Transpose, change instrument, edit tempo |

## Patch Format

```
┌──────────────────────────────────────────────┐
│  PatchHeader (8 bytes)                        │
│  ├─ magic: [u8; 4]     = "AVCS"             │
│  ├─ base_hash: [u8; 4] = truncated BLAKE3   │
│  ├─ op_count: u16      = number of ops       │
│  └─ ast_type: u8       = SDF/Scene/Panel     │
├──────────────────────────────────────────────┤
│  Operation (4-12 bytes each)                  │
│  ├─ op_type: u8     = Insert/Delete/Update   │
│  ├─ node_id: u16    = target node            │
│  ├─ field: u8       = which field (if Update)│
│  └─ value: [u8; N]  = new value (variable)   │
└──────────────────────────────────────────────┘
```

### Size Comparison

| Change | Git Binary Diff | ALICE-VCS Patch | Ratio |
|--------|----------------|----------------|-------|
| Change sphere radius | ~2 KB | **12 bytes** | **170x** |
| Move node in scene | ~50 KB | **16 bytes** | **3,125x** |
| Add CSG subtraction | ~5 KB | **20 bytes** | **250x** |
| Recolor character | ~10 KB | **8 bytes** | **1,250x** |
| Full episode edit (50 changes) | ~500 KB | **~600 bytes** | **833x** |

## Merge Strategy

### 3-Way Structural Merge

```
           Base (AST₀)
          /            \
    Ours (AST₁)    Theirs (AST₂)
          \            /
         Merged (AST₃)
```

| Scenario | Resolution |
|----------|-----------|
| Non-overlapping subtrees | Auto-merge (no conflict) |
| Same node, different fields | Auto-merge (field-level) |
| Same node, same field | **Conflict** → manual resolve |
| Insert at same position | **Conflict** → manual resolve |
| Delete vs. modify same node | **Conflict** → manual resolve |

## API Design

```rust
use alice_vcs::{Repository, Commit, Diff, Patch};

// Initialize repository
let repo = Repository::open("./my_scene")?;

// Compute semantic diff between two AST snapshots
let diff = Diff::compute(&old_ast, &new_ast)?;
// diff.operations = [Update(node=5, field=radius, value=1.5)]
// diff.size_bytes() = 12

// Create commit
let commit = repo.commit(
    parent: &head,
    patch: &diff.to_patch(),
    message: "Scale sphere radius to 1.5",
)?;

// Merge branches (3-way structural)
let merge_result = repo.merge(&branch_a, &branch_b)?;
match merge_result {
    MergeResult::Clean(merged_ast) => { /* auto-merged */ }
    MergeResult::Conflict(conflicts) => { /* manual resolve */ }
}

// P2P sync (via ALICE-Sync)
repo.push(&remote)?;  // Send patches (bytes, not full files)
repo.pull(&remote)?;   // Receive and apply patches
```

## Ecosystem Integration

```
ALICE-SDF ──── AST ────▶ ALICE-VCS ──── Patches ────▶ ALICE-Sync
ALICE-Animation ─ AST ─▶     │                             │
ALICE-Manga ──── AST ──▶     │                             ▼
                              ▼                        ALICE-DB
                         ALICE-Auth                  (snapshot store)
                      (commit signing)
```

| Bridge | Direction | Data |
|--------|-----------|------|
| SDF → VCS | Parse CSG tree into diffable AST | SdfNode tree |
| Animation → VCS | Parse SceneGraph into diffable AST | Actor/Timeline tree |
| VCS → Sync | Distribute patches via P2P event diffing | Patch bytes |
| VCS → DB | Persist snapshots and commit history | Compressed AST |
| VCS → Auth | Ed25519 commit signing and verification | Signature (64 bytes) |

## Feature Flags

| Feature | Dependencies | Description |
|---------|-------------|-------------|
| *(default)* | None | Core diff engine, no_std compatible |
| `std` | std | File I/O, repository management |
| `sdf` | alice-sdf | ALICE-SDF AST diffing |
| `sync` | alice-sync | P2P patch replication |
| `db` | alice-db | Snapshot persistence |
| `auth` | alice-auth | Commit signing (Ed25519) |

## License

AGPL-3.0

## Author

Moroya Sakamoto

---

*"Version control should understand structure, not just text."*
