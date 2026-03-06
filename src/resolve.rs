//! 競合解決モジュール
//!
//! 3-way マージで検出されたコンフリクトに対し、
//! 自動解決戦略 (Ours/Theirs/Union/Drop) を適用する。
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::diff::DiffOp;
use crate::merge::{Conflict, MergeResult};

/// 競合解決戦略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionStrategy {
    /// ブランチ A の操作を採用。
    Ours,
    /// ブランチ B の操作を採用。
    Theirs,
    /// 両方の操作を結合 (A を先、B を後)。
    ///
    /// ノード削除と更新が競合する場合は危険な場合がある。
    Union,
    /// 両方の操作を破棄 (ベースのまま維持)。
    Drop,
}

/// 単一のコンフリクトを解決し、適用すべき操作列を返す。
#[must_use]
pub fn resolve_conflict(conflict: &Conflict, strategy: ResolutionStrategy) -> Vec<DiffOp> {
    match strategy {
        ResolutionStrategy::Ours => conflict.ops_a.clone(),
        ResolutionStrategy::Theirs => conflict.ops_b.clone(),
        ResolutionStrategy::Union => {
            let mut ops = conflict.ops_a.clone();
            ops.extend(conflict.ops_b.iter().cloned());
            ops
        }
        ResolutionStrategy::Drop => Vec::new(),
    }
}

/// 全コンフリクトに同一の戦略を適用し、解決済みの操作列を返す。
#[must_use]
pub fn resolve_all(conflicts: &[Conflict], strategy: ResolutionStrategy) -> Vec<DiffOp> {
    let mut ops = Vec::new();
    for conflict in conflicts {
        ops.extend(resolve_conflict(conflict, strategy));
    }
    ops
}

/// `MergeResult` を完全に解決し、適用可能な操作列に変換する。
///
/// `merged_ops` (非競合) + 解決済みコンフリクト操作を結合して返す。
#[must_use]
pub fn resolve_merge(result: &MergeResult, strategy: ResolutionStrategy) -> Vec<DiffOp> {
    let mut ops = result.merged_ops.clone();
    ops.extend(resolve_all(&result.conflicts, strategy));
    ops
}

/// コンフリクトごとに異なる戦略を指定して解決する。
///
/// `strategies` の長さが `conflicts` より短い場合、
/// 残りのコンフリクトには `default_strategy` が適用される。
#[must_use]
pub fn resolve_selective(
    result: &MergeResult,
    strategies: &[ResolutionStrategy],
    default_strategy: ResolutionStrategy,
) -> Vec<DiffOp> {
    let mut ops = result.merged_ops.clone();
    for (i, conflict) in result.conflicts.iter().enumerate() {
        let strategy = strategies.get(i).copied().unwrap_or(default_strategy);
        ops.extend(resolve_conflict(conflict, strategy));
    }
    ops
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::NodeValue;
    use crate::diff::DiffOp;
    use crate::merge::merge_patches;
    #[cfg(not(feature = "std"))]
    use alloc::{format, string::String, vec};

    fn make_conflict() -> Conflict {
        Conflict {
            node_id: 1,
            description: String::from("test conflict"),
            ops_a: vec![DiffOp::Update {
                node_id: 1,
                old_value: NodeValue::Float(1.0),
                new_value: NodeValue::Float(2.0),
            }],
            ops_b: vec![DiffOp::Update {
                node_id: 1,
                old_value: NodeValue::Float(1.0),
                new_value: NodeValue::Float(3.0),
            }],
        }
    }

    // --- resolve_conflict ---

    #[test]
    fn resolve_ours() {
        let conflict = make_conflict();
        let ops = resolve_conflict(&conflict, ResolutionStrategy::Ours);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], DiffOp::Update { new_value, .. } if *new_value == NodeValue::Float(2.0))
        );
    }

    #[test]
    fn resolve_theirs() {
        let conflict = make_conflict();
        let ops = resolve_conflict(&conflict, ResolutionStrategy::Theirs);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], DiffOp::Update { new_value, .. } if *new_value == NodeValue::Float(3.0))
        );
    }

    #[test]
    fn resolve_union() {
        let conflict = make_conflict();
        let ops = resolve_conflict(&conflict, ResolutionStrategy::Union);
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn resolve_drop() {
        let conflict = make_conflict();
        let ops = resolve_conflict(&conflict, ResolutionStrategy::Drop);
        assert!(ops.is_empty());
    }

    // --- resolve_all ---

    #[test]
    fn resolve_all_ours() {
        let conflicts = vec![make_conflict(), make_conflict()];
        let ops = resolve_all(&conflicts, ResolutionStrategy::Ours);
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn resolve_all_empty() {
        let ops = resolve_all(&[], ResolutionStrategy::Ours);
        assert!(ops.is_empty());
    }

    // --- resolve_merge ---

    #[test]
    fn resolve_merge_clean() {
        let patch_a = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let patch_b = vec![DiffOp::Update {
            node_id: 2,
            old_value: NodeValue::Float(3.0),
            new_value: NodeValue::Float(4.0),
        }];
        let result = merge_patches(&patch_a, &patch_b);
        let ops = resolve_merge(&result, ResolutionStrategy::Ours);
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn resolve_merge_with_conflict_ours() {
        let patch_a = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let patch_b = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(3.0),
        }];
        let result = merge_patches(&patch_a, &patch_b);
        assert!(!result.is_clean());

        let ops = resolve_merge(&result, ResolutionStrategy::Ours);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], DiffOp::Update { new_value, .. } if *new_value == NodeValue::Float(2.0))
        );
    }

    #[test]
    fn resolve_merge_with_conflict_theirs() {
        let patch_a = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let patch_b = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(3.0),
        }];
        let result = merge_patches(&patch_a, &patch_b);
        let ops = resolve_merge(&result, ResolutionStrategy::Theirs);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], DiffOp::Update { new_value, .. } if *new_value == NodeValue::Float(3.0))
        );
    }

    #[test]
    fn resolve_merge_with_conflict_drop() {
        let patch_a = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let patch_b = vec![DiffOp::Update {
            node_id: 1,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(3.0),
        }];
        let result = merge_patches(&patch_a, &patch_b);
        let ops = resolve_merge(&result, ResolutionStrategy::Drop);
        // No merged_ops (both sides conflicted), no resolved ops
        assert!(ops.is_empty());
    }

    // --- resolve_selective ---

    #[test]
    fn resolve_selective_mixed() {
        let patch_a = vec![
            DiffOp::Update {
                node_id: 1,
                old_value: NodeValue::Int(0),
                new_value: NodeValue::Int(1),
            },
            DiffOp::Update {
                node_id: 2,
                old_value: NodeValue::Int(0),
                new_value: NodeValue::Int(10),
            },
        ];
        let patch_b = vec![
            DiffOp::Update {
                node_id: 1,
                old_value: NodeValue::Int(0),
                new_value: NodeValue::Int(2),
            },
            DiffOp::Update {
                node_id: 2,
                old_value: NodeValue::Int(0),
                new_value: NodeValue::Int(20),
            },
        ];
        let result = merge_patches(&patch_a, &patch_b);
        assert_eq!(result.conflicts.len(), 2);

        // 1つ目は Ours、2つ目は Theirs
        let strategies = [ResolutionStrategy::Ours, ResolutionStrategy::Theirs];
        let ops = resolve_selective(&result, &strategies, ResolutionStrategy::Drop);
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn resolve_selective_default_strategy() {
        let patch_a = vec![
            DiffOp::Delete { node_id: 10 },
            DiffOp::Delete { node_id: 20 },
        ];
        let patch_b = vec![
            DiffOp::Update {
                node_id: 10,
                old_value: NodeValue::Int(0),
                new_value: NodeValue::Int(1),
            },
            DiffOp::Update {
                node_id: 20,
                old_value: NodeValue::Int(0),
                new_value: NodeValue::Int(2),
            },
        ];
        let result = merge_patches(&patch_a, &patch_b);

        // 戦略を1つだけ指定 → 2つ目はデフォルト (Drop)
        let strategies = [ResolutionStrategy::Ours];
        let ops = resolve_selective(&result, &strategies, ResolutionStrategy::Drop);
        // 1つ目: Ours (Delete node 10) → 1 op
        // 2つ目: Drop → 0 ops
        assert_eq!(ops.len(), 1);
    }

    // --- Strategy Debug/Clone/Eq ---

    #[test]
    fn strategy_debug_and_eq() {
        assert_eq!(ResolutionStrategy::Ours, ResolutionStrategy::Ours);
        assert_ne!(ResolutionStrategy::Ours, ResolutionStrategy::Theirs);
        let dbg = format!("{:?}", ResolutionStrategy::Union);
        assert_eq!(dbg, "Union");
    }

    #[test]
    fn strategy_copy() {
        let s = ResolutionStrategy::Drop;
        let s2 = s;
        assert_eq!(s, s2);
    }
}
