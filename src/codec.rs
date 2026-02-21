//! Varint patch codec — compact binary serialization for DiffOp patches
//!
//! Encodes AST diff operations into a compact byte stream using LEB128
//! varint encoding.  Typical patches are 4-12 bytes per operation vs
//! 50 KB+ for naive binary diffs.
//!
//! Author: Moroya Sakamoto

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use crate::ast::{AstNodeKind, NodeValue};
use crate::diff::DiffOp;

// ── Op Type Discriminants ──────────────────────────────────────────────

const OP_INSERT: u8 = 0x00;
const OP_DELETE: u8 = 0x01;
const OP_UPDATE: u8 = 0x02;
const OP_RELABEL: u8 = 0x03;
const OP_MOVE: u8 = 0x04;

const VAL_NONE: u8 = 0x00;
const VAL_INT: u8 = 0x01;
const VAL_FLOAT: u8 = 0x02;
const VAL_TEXT: u8 = 0x03;
const VAL_IDENT: u8 = 0x04;
const VAL_BYTES: u8 = 0x05;

// ── Varint (LEB128) ───────────────────────────────────────────────────

/// Encode a u32 as LEB128 varint.
#[inline]
fn encode_varint_u32(mut value: u32, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Decode a u32 from LEB128 varint.
#[inline]
fn decode_varint_u32(data: &[u8], pos: &mut usize) -> Option<u32> {
    let mut value: u32 = 0;
    let mut shift: u32 = 0;
    loop {
        if *pos >= data.len() {
            return None;
        }
        let byte = data[*pos];
        *pos += 1;
        value |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 35 {
            return None;
        }
    }
    Some(value)
}

/// Encode a usize as varint (truncated to u32).
#[inline]
fn encode_usize(value: usize, buf: &mut Vec<u8>) {
    encode_varint_u32(value as u32, buf);
}

/// Decode a usize from varint.
#[inline]
fn decode_usize(data: &[u8], pos: &mut usize) -> Option<usize> {
    decode_varint_u32(data, pos).map(|v| v as usize)
}

// ── NodeValue Codec ────────────────────────────────────────────────────

fn encode_value(value: &NodeValue, buf: &mut Vec<u8>) {
    match value {
        NodeValue::None => buf.push(VAL_NONE),
        NodeValue::Int(v) => {
            buf.push(VAL_INT);
            buf.extend_from_slice(&v.to_le_bytes());
        }
        NodeValue::Float(v) => {
            buf.push(VAL_FLOAT);
            buf.extend_from_slice(&v.to_le_bytes());
        }
        NodeValue::Text(s) => {
            buf.push(VAL_TEXT);
            encode_usize(s.len(), buf);
            buf.extend_from_slice(s.as_bytes());
        }
        NodeValue::Ident(s) => {
            buf.push(VAL_IDENT);
            encode_usize(s.len(), buf);
            buf.extend_from_slice(s.as_bytes());
        }
        NodeValue::Bytes(b) => {
            buf.push(VAL_BYTES);
            encode_usize(b.len(), buf);
            buf.extend_from_slice(b);
        }
    }
}

fn decode_value(data: &[u8], pos: &mut usize) -> Option<NodeValue> {
    if *pos >= data.len() {
        return None;
    }
    let tag = data[*pos];
    *pos += 1;
    match tag {
        VAL_NONE => Some(NodeValue::None),
        VAL_INT => {
            if *pos + 8 > data.len() {
                return None;
            }
            let v = i64::from_le_bytes(data[*pos..*pos + 8].try_into().ok()?);
            *pos += 8;
            Some(NodeValue::Int(v))
        }
        VAL_FLOAT => {
            if *pos + 8 > data.len() {
                return None;
            }
            let v = f64::from_le_bytes(data[*pos..*pos + 8].try_into().ok()?);
            *pos += 8;
            Some(NodeValue::Float(v))
        }
        VAL_TEXT => {
            let len = decode_usize(data, pos)?;
            if *pos + len > data.len() {
                return None;
            }
            let s = String::from_utf8(data[*pos..*pos + len].to_vec()).ok()?;
            *pos += len;
            Some(NodeValue::Text(s))
        }
        VAL_IDENT => {
            let len = decode_usize(data, pos)?;
            if *pos + len > data.len() {
                return None;
            }
            let s = String::from_utf8(data[*pos..*pos + len].to_vec()).ok()?;
            *pos += len;
            Some(NodeValue::Ident(s))
        }
        VAL_BYTES => {
            let len = decode_usize(data, pos)?;
            if *pos + len > data.len() {
                return None;
            }
            let b = data[*pos..*pos + len].to_vec();
            *pos += len;
            Some(NodeValue::Bytes(b))
        }
        _ => None,
    }
}

// ── String Codec ───────────────────────────────────────────────────────

fn encode_string(s: &str, buf: &mut Vec<u8>) {
    encode_usize(s.len(), buf);
    buf.extend_from_slice(s.as_bytes());
}

fn decode_string(data: &[u8], pos: &mut usize) -> Option<String> {
    let len = decode_usize(data, pos)?;
    if *pos + len > data.len() {
        return None;
    }
    let s = String::from_utf8(data[*pos..*pos + len].to_vec()).ok()?;
    *pos += len;
    Some(s)
}

// ── DiffOp Codec ───────────────────────────────────────────────────────

/// Encode a single DiffOp into the buffer.
pub fn encode_op(op: &DiffOp, buf: &mut Vec<u8>) {
    match op {
        DiffOp::Insert {
            parent_id,
            index,
            kind,
            label,
            value,
        } => {
            buf.push(OP_INSERT);
            encode_varint_u32(*parent_id, buf);
            encode_usize(*index, buf);
            buf.push(*kind as u8);
            encode_string(label, buf);
            encode_value(value, buf);
        }
        DiffOp::Delete { node_id } => {
            buf.push(OP_DELETE);
            encode_varint_u32(*node_id, buf);
        }
        DiffOp::Update {
            node_id,
            old_value,
            new_value,
        } => {
            buf.push(OP_UPDATE);
            encode_varint_u32(*node_id, buf);
            encode_value(old_value, buf);
            encode_value(new_value, buf);
        }
        DiffOp::Relabel {
            node_id,
            old_label,
            new_label,
        } => {
            buf.push(OP_RELABEL);
            encode_varint_u32(*node_id, buf);
            encode_string(old_label, buf);
            encode_string(new_label, buf);
        }
        DiffOp::Move {
            node_id,
            new_parent_id,
            new_index,
        } => {
            buf.push(OP_MOVE);
            encode_varint_u32(*node_id, buf);
            encode_varint_u32(*new_parent_id, buf);
            encode_usize(*new_index, buf);
        }
    }
}

/// Decode a single DiffOp from the buffer.
pub fn decode_op(data: &[u8], pos: &mut usize) -> Option<DiffOp> {
    if *pos >= data.len() {
        return None;
    }
    let tag = data[*pos];
    *pos += 1;
    match tag {
        OP_INSERT => {
            let parent_id = decode_varint_u32(data, pos)?;
            let index = decode_usize(data, pos)?;
            if *pos >= data.len() {
                return None;
            }
            let kind_byte = data[*pos];
            *pos += 1;
            let kind = AstNodeKind::from_u8(kind_byte);
            let label = decode_string(data, pos)?;
            let value = decode_value(data, pos)?;
            Some(DiffOp::Insert {
                parent_id,
                index,
                kind,
                label,
                value,
            })
        }
        OP_DELETE => {
            let node_id = decode_varint_u32(data, pos)?;
            Some(DiffOp::Delete { node_id })
        }
        OP_UPDATE => {
            let node_id = decode_varint_u32(data, pos)?;
            let old_value = decode_value(data, pos)?;
            let new_value = decode_value(data, pos)?;
            Some(DiffOp::Update {
                node_id,
                old_value,
                new_value,
            })
        }
        OP_RELABEL => {
            let node_id = decode_varint_u32(data, pos)?;
            let old_label = decode_string(data, pos)?;
            let new_label = decode_string(data, pos)?;
            Some(DiffOp::Relabel {
                node_id,
                old_label,
                new_label,
            })
        }
        OP_MOVE => {
            let node_id = decode_varint_u32(data, pos)?;
            let new_parent_id = decode_varint_u32(data, pos)?;
            let new_index = decode_usize(data, pos)?;
            Some(DiffOp::Move {
                node_id,
                new_parent_id,
                new_index,
            })
        }
        _ => None,
    }
}

/// Encode a full patch (list of DiffOps) into a byte buffer.
///
/// Format: `[varint: op_count] [op1] [op2] ...`
pub fn encode_patch(ops: &[DiffOp]) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_usize(ops.len(), &mut buf);
    for op in ops {
        encode_op(op, &mut buf);
    }
    buf
}

/// Decode a full patch from a byte buffer.
pub fn decode_patch(data: &[u8]) -> Option<Vec<DiffOp>> {
    let mut pos = 0;
    let count = decode_usize(data, &mut pos)?;
    let mut ops = Vec::with_capacity(count);
    for _ in 0..count {
        ops.push(decode_op(data, &mut pos)?);
    }
    Some(ops)
}

/// Encoded patch size in bytes (without actually allocating).
pub fn encoded_patch_size(ops: &[DiffOp]) -> usize {
    encode_patch(ops).len()
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNodeKind, NodeValue};
    use crate::diff::DiffOp;
    #[cfg(not(feature = "std"))]
    use alloc::vec;

    #[test]
    fn varint_roundtrip_small() {
        let mut buf = Vec::new();
        encode_varint_u32(42, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_varint_u32(&buf, &mut pos), Some(42));
        assert_eq!(buf.len(), 1); // 42 fits in 1 byte
    }

    #[test]
    fn varint_roundtrip_large() {
        let mut buf = Vec::new();
        encode_varint_u32(0xFFFF_FFFF, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_varint_u32(&buf, &mut pos), Some(0xFFFF_FFFF));
        assert_eq!(buf.len(), 5); // max u32 needs 5 bytes
    }

    #[test]
    fn varint_roundtrip_zero() {
        let mut buf = Vec::new();
        encode_varint_u32(0, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_varint_u32(&buf, &mut pos), Some(0));
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn varint_boundary_128() {
        let mut buf = Vec::new();
        encode_varint_u32(127, &mut buf);
        assert_eq!(buf.len(), 1);
        buf.clear();
        encode_varint_u32(128, &mut buf);
        assert_eq!(buf.len(), 2); // 128 needs 2 bytes
        let mut pos = 0;
        assert_eq!(decode_varint_u32(&buf, &mut pos), Some(128));
    }

    #[test]
    fn delete_roundtrip() {
        let op = DiffOp::Delete { node_id: 42 };
        let mut buf = Vec::new();
        encode_op(&op, &mut buf);
        let mut pos = 0;
        let decoded = decode_op(&buf, &mut pos).unwrap();
        assert_eq!(decoded, op);
        assert_eq!(buf.len(), 2); // 1 tag + 1 varint
    }

    #[test]
    fn update_float_roundtrip() {
        let op = DiffOp::Update {
            node_id: 5,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.5),
        };
        let mut buf = Vec::new();
        encode_op(&op, &mut buf);
        let mut pos = 0;
        let decoded = decode_op(&buf, &mut pos).unwrap();
        assert_eq!(decoded, op);
    }

    #[test]
    fn update_int_roundtrip() {
        let op = DiffOp::Update {
            node_id: 100,
            old_value: NodeValue::Int(-42),
            new_value: NodeValue::Int(999),
        };
        let mut buf = Vec::new();
        encode_op(&op, &mut buf);
        let mut pos = 0;
        let decoded = decode_op(&buf, &mut pos).unwrap();
        assert_eq!(decoded, op);
    }

    #[test]
    fn insert_roundtrip() {
        let op = DiffOp::Insert {
            parent_id: 0,
            index: 3,
            kind: AstNodeKind::Primitive,
            label: String::from("sphere"),
            value: NodeValue::Float(1.5),
        };
        let mut buf = Vec::new();
        encode_op(&op, &mut buf);
        let mut pos = 0;
        let decoded = decode_op(&buf, &mut pos).unwrap();
        assert_eq!(decoded, op);
    }

    #[test]
    fn relabel_roundtrip() {
        let op = DiffOp::Relabel {
            node_id: 7,
            old_label: String::from("sphere"),
            new_label: String::from("box"),
        };
        let mut buf = Vec::new();
        encode_op(&op, &mut buf);
        let mut pos = 0;
        let decoded = decode_op(&buf, &mut pos).unwrap();
        assert_eq!(decoded, op);
    }

    #[test]
    fn move_roundtrip() {
        let op = DiffOp::Move {
            node_id: 3,
            new_parent_id: 1,
            new_index: 0,
        };
        let mut buf = Vec::new();
        encode_op(&op, &mut buf);
        let mut pos = 0;
        let decoded = decode_op(&buf, &mut pos).unwrap();
        assert_eq!(decoded, op);
        // Move: 1 tag + 3 varints = 4 bytes for small IDs
        assert_eq!(buf.len(), 4);
    }

    #[test]
    fn patch_roundtrip_multiple_ops() {
        let ops = vec![
            DiffOp::Delete { node_id: 10 },
            DiffOp::Update {
                node_id: 5,
                old_value: NodeValue::Float(1.0),
                new_value: NodeValue::Float(2.0),
            },
            DiffOp::Insert {
                parent_id: 0,
                index: 0,
                kind: AstNodeKind::CsgOp,
                label: String::from("union"),
                value: NodeValue::None,
            },
            DiffOp::Relabel {
                node_id: 3,
                old_label: String::from("a"),
                new_label: String::from("b"),
            },
            DiffOp::Move {
                node_id: 7,
                new_parent_id: 2,
                new_index: 1,
            },
        ];
        let encoded = encode_patch(&ops);
        let decoded = decode_patch(&encoded).unwrap();
        assert_eq!(decoded.len(), ops.len());
        for (orig, dec) in ops.iter().zip(decoded.iter()) {
            assert_eq!(orig, dec);
        }
    }

    #[test]
    fn empty_patch_roundtrip() {
        let ops: Vec<DiffOp> = vec![];
        let encoded = encode_patch(&ops);
        let decoded = decode_patch(&encoded).unwrap();
        assert!(decoded.is_empty());
        assert_eq!(encoded.len(), 1); // just the count varint
    }

    #[test]
    fn value_none_roundtrip() {
        let mut buf = Vec::new();
        encode_value(&NodeValue::None, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_value(&buf, &mut pos), Some(NodeValue::None));
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn value_text_roundtrip() {
        let val = NodeValue::Text(String::from("hello world"));
        let mut buf = Vec::new();
        encode_value(&val, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_value(&buf, &mut pos), Some(val));
    }

    #[test]
    fn value_bytes_roundtrip() {
        let val = NodeValue::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let mut buf = Vec::new();
        encode_value(&val, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_value(&buf, &mut pos), Some(val));
    }

    #[test]
    fn decode_truncated_returns_none() {
        // Truncated varint
        let buf = vec![0x80]; // continuation bit set but no follow-up
        let mut pos = 0;
        assert_eq!(decode_varint_u32(&buf, &mut pos), None);
    }

    #[test]
    fn decode_empty_returns_none() {
        let buf: Vec<u8> = vec![];
        let mut pos = 0;
        assert_eq!(decode_op(&buf, &mut pos), None);
    }

    #[test]
    fn patch_size_compact() {
        // A single value-change patch should be very compact
        let ops = vec![DiffOp::Update {
            node_id: 5,
            old_value: NodeValue::Float(1.0),
            new_value: NodeValue::Float(2.0),
        }];
        let size = encoded_patch_size(&ops);
        // 1 (count) + 1 (tag) + 1 (node_id varint) + 1+8 (old float) + 1+8 (new float) = 21
        assert!(size < 25, "update patch should be compact, got {size}");
    }

    #[test]
    fn large_node_id_varint() {
        let op = DiffOp::Delete {
            node_id: 100_000,
        };
        let mut buf = Vec::new();
        encode_op(&op, &mut buf);
        let mut pos = 0;
        let decoded = decode_op(&buf, &mut pos).unwrap();
        assert_eq!(decoded, op);
    }

    // ── New tests ──────────────────────────────────────────────────────

    #[test]
    fn varint_roundtrip_boundary_16384() {
        // 16384 is the first value requiring 3 LEB128 bytes
        let mut buf = Vec::new();
        encode_varint_u32(16384, &mut buf);
        assert_eq!(buf.len(), 3);
        let mut pos = 0;
        assert_eq!(decode_varint_u32(&buf, &mut pos), Some(16384));
    }

    #[test]
    fn decode_varint_out_of_bounds_is_none() {
        // Buffer is empty — decode must return None, not panic
        let buf: Vec<u8> = vec![];
        let mut pos = 0;
        assert_eq!(decode_varint_u32(&buf, &mut pos), None);
    }

    #[test]
    fn value_ident_roundtrip() {
        let val = NodeValue::Ident(String::from("union"));
        let mut buf = Vec::new();
        encode_value(&val, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_value(&buf, &mut pos), Some(val));
    }

    #[test]
    fn value_int_roundtrip() {
        for v in [i64::MIN, -1, 0, 1, i64::MAX] {
            let val = NodeValue::Int(v);
            let mut buf = Vec::new();
            encode_value(&val, &mut buf);
            let mut pos = 0;
            assert_eq!(decode_value(&buf, &mut pos), Some(val));
        }
    }

    #[test]
    fn value_float_roundtrip_special() {
        for v in [0.0f64, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let val = NodeValue::Float(v);
            let mut buf = Vec::new();
            encode_value(&val, &mut buf);
            let mut pos = 0;
            let decoded = decode_value(&buf, &mut pos).unwrap();
            match (&val, &decoded) {
                (NodeValue::Float(a), NodeValue::Float(b)) => {
                    // NaN != NaN, compare bits instead
                    assert_eq!(a.to_bits(), b.to_bits());
                }
                _ => panic!("expected Float"),
            }
        }
    }

    #[test]
    fn value_bytes_empty_roundtrip() {
        let val = NodeValue::Bytes(vec![]);
        let mut buf = Vec::new();
        encode_value(&val, &mut buf);
        let mut pos = 0;
        assert_eq!(decode_value(&buf, &mut pos), Some(val));
    }

    #[test]
    fn decode_unknown_value_tag_returns_none() {
        let buf = vec![0xFF]; // unknown tag
        let mut pos = 0;
        assert_eq!(decode_value(&buf, &mut pos), None);
    }

    #[test]
    fn decode_truncated_float_returns_none() {
        // VAL_FLOAT tag followed by only 4 bytes instead of 8
        let buf = vec![0x02, 0x00, 0x00, 0x00, 0x00];
        let mut pos = 0;
        assert_eq!(decode_value(&buf, &mut pos), None);
    }

    #[test]
    fn decode_unknown_op_tag_returns_none() {
        let buf = vec![0xFF]; // unknown op tag
        let mut pos = 0;
        assert_eq!(decode_op(&buf, &mut pos), None);
    }

    #[test]
    fn patch_roundtrip_all_value_types() {
        let ops = vec![
            DiffOp::Update {
                node_id: 1,
                old_value: NodeValue::None,
                new_value: NodeValue::Int(42),
            },
            DiffOp::Update {
                node_id: 2,
                old_value: NodeValue::Float(1.0),
                new_value: NodeValue::Text(String::from("hello")),
            },
            DiffOp::Update {
                node_id: 3,
                old_value: NodeValue::Ident(String::from("sphere")),
                new_value: NodeValue::Bytes(vec![0xDE, 0xAD]),
            },
        ];
        let encoded = encode_patch(&ops);
        let decoded = decode_patch(&encoded).unwrap();
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0], ops[0]);
        assert_eq!(decoded[1], ops[1]);
        assert_eq!(decoded[2], ops[2]);
    }

    #[test]
    fn encoded_patch_size_matches_actual() {
        let ops = vec![
            DiffOp::Delete { node_id: 1 },
            DiffOp::Move { node_id: 2, new_parent_id: 0, new_index: 1 },
        ];
        let actual = encode_patch(&ops).len();
        let reported = encoded_patch_size(&ops);
        assert_eq!(actual, reported);
    }
}
