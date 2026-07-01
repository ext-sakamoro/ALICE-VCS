//! `signed_commit` — cryptographic attestation for VCS commits.
//!
//! Wraps a VCS commit record (author, timestamp, tree hash, parent hashes,
//! message) in an `Ed25519` signature so code reviewers can verify who
//! authored each commit, independent of the transport (SSH remote, HTTP
//! mirror, offline patch).
//!
//! # Regulatory alignment
//!
//! - **`SOX` §404** — code changes to systems that touch financial
//!   reporting must have traceable authorship; the signature binds the
//!   commit to a specific key holder.
//! - **`HIPAA Security Rule` §164.312(c)** — integrity controls for
//!   electronic protected health information; signed commits prevent
//!   silent modification of software that processes `PHI`.
//! - **`FedRAMP` `CM-3`, `CM-5`** — configuration management change
//!   control and access restrictions.
//! - **`SLSA` Level 3** — signed provenance of build artifacts starts
//!   with signed source commits.
//! - **`SOC2 CC7.1`** — the entity uses detection and monitoring
//!   procedures; git-style signed commits are an accepted control.
//!
//! Cryptographic primitives are provided by `alice-blockchain` (`Ed25519`).
//!
//! This module requires the `std` feature.

#![allow(
    clippy::doc_markdown,
    clippy::missing_panics_doc,
    clippy::too_many_arguments,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation
)]

use alice_blockchain::signature::{KeyPair, PublicKey, Signature};

// ---------------------------------------------------------------------------
// CommitPayload
// ---------------------------------------------------------------------------

/// The immutable payload of a commit — signed as one blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPayload {
    /// Author identity string (`Name <email>` convention).
    pub author: String,
    /// Author email (extracted for indexing / audit reports).
    pub email: String,
    /// Unix nanosecond timestamp of the commit.
    pub timestamp_ns: u64,
    /// Root tree hash after the commit is applied (hex-encoded string).
    pub tree_hash: String,
    /// Zero or more parent commit hashes (empty for the root commit,
    /// multiple for merges).
    pub parent_hashes: Vec<String>,
    /// Commit message.
    pub message: String,
}

impl CommitPayload {
    /// Canonical byte layout used for hashing and signing.
    ///
    /// Parents are joined with a NUL byte and terminated with a sentinel
    /// NUL to distinguish `[a, b]` from `[ab]`.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(self.author.as_bytes());
        buf.push(0);
        buf.extend_from_slice(self.email.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.extend_from_slice(self.tree_hash.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&(self.parent_hashes.len() as u64).to_le_bytes());
        for parent in &self.parent_hashes {
            buf.extend_from_slice(parent.as_bytes());
            buf.push(0);
        }
        buf.extend_from_slice(self.message.as_bytes());
        buf.push(0);
        buf
    }

    /// `FNV-1a` hash of the canonical byte layout. This is the commit id.
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in &self.canonical_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}

// ---------------------------------------------------------------------------
// SignedCommit
// ---------------------------------------------------------------------------

/// [`CommitPayload`] plus the author's `Ed25519` signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedCommit {
    /// The wrapped commit payload.
    pub payload: CommitPayload,
    /// `FNV-1a` hash (= commit id) of the payload.
    pub hash: u64,
    /// `Ed25519` signature over the canonical bytes.
    pub signature: Signature,
    /// Author's `Ed25519` public key.
    pub author_key: PublicKey,
}

impl SignedCommit {
    /// Sign a commit payload with the author's key pair.
    #[must_use]
    pub fn sign(keypair: &KeyPair, payload: CommitPayload) -> Self {
        let bytes = payload.canonical_bytes();
        let hash = payload.hash();
        let signature = keypair.sign(&bytes);
        let author_key = keypair.public();
        Self {
            payload,
            hash,
            signature,
            author_key,
        }
    }

    /// Verify the signature and hash consistency.
    #[must_use]
    pub fn verify(&self) -> bool {
        if self.hash != self.payload.hash() {
            return false;
        }
        self.author_key
            .verify(&self.payload.canonical_bytes(), &self.signature)
    }
}

// ---------------------------------------------------------------------------
// CommitChain
// ---------------------------------------------------------------------------

/// A linear chain of signed commits. Non-linear DAG histories can be
/// represented by combining multiple chains through a merge commit whose
/// `parent_hashes` reference both tips.
#[derive(Debug, Clone, Default)]
pub struct CommitChain {
    commits: Vec<SignedCommit>,
}

impl CommitChain {
    /// Construct an empty chain.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commits: Vec::new(),
        }
    }

    /// Number of commits.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.commits.len()
    }

    /// Whether the chain is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    /// Read-only view.
    #[must_use]
    pub fn commits(&self) -> &[SignedCommit] {
        &self.commits
    }

    /// Hash of the tip commit (0 for empty chain).
    #[must_use]
    pub fn tip_hash(&self) -> u64 {
        self.commits.last().map_or(0, |c| c.hash)
    }

    /// Append a new commit signed by the author's key pair. The commit's
    /// `parent_hashes` is overwritten to reference the current tip.
    pub fn commit(
        &mut self,
        keypair: &KeyPair,
        author: impl Into<String>,
        email: impl Into<String>,
        timestamp_ns: u64,
        tree_hash: impl Into<String>,
        message: impl Into<String>,
    ) -> &SignedCommit {
        let parent_hashes = if self.commits.is_empty() {
            Vec::new()
        } else {
            vec![format!("{:016x}", self.tip_hash())]
        };
        let payload = CommitPayload {
            author: author.into(),
            email: email.into(),
            timestamp_ns,
            tree_hash: tree_hash.into(),
            parent_hashes,
            message: message.into(),
        };
        let signed = SignedCommit::sign(keypair, payload);
        self.commits.push(signed);
        self.commits.last().expect("commit was just pushed")
    }

    /// Verify every commit's signature and its `parent_hashes[0]`
    /// against the previous commit's hash.
    ///
    /// Returns the index of the first invalid commit, or `None` if the
    /// chain is intact.
    #[must_use]
    pub fn find_first_invalid(&self) -> Option<usize> {
        let mut prev_id = String::new();
        for (i, c) in self.commits.iter().enumerate() {
            if !c.verify() {
                return Some(i);
            }
            if i == 0 {
                if !c.payload.parent_hashes.is_empty() {
                    return Some(i);
                }
            } else if c.payload.parent_hashes.first() != Some(&prev_id) {
                return Some(i);
            }
            prev_id = format!("{:016x}", c.hash);
        }
        None
    }

    /// Whether the chain is intact end-to-end.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.find_first_invalid().is_none()
    }

    /// Distinct authors seen in the chain.
    #[must_use]
    pub fn authors(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.commits {
            if !out.contains(&c.payload.author) {
                out.push(c.payload.author.clone());
            }
        }
        out
    }

    /// Count commits authored by the given identity.
    #[must_use]
    pub fn commits_by(&self, author: &str) -> usize {
        self.commits
            .iter()
            .filter(|c| c.payload.author == author)
            .count()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn kp(seed: u8) -> KeyPair {
        KeyPair::from_seed([seed; 32])
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let p = CommitPayload {
            author: String::from("Alice"),
            email: String::from("alice@example.com"),
            timestamp_ns: 1_000_000,
            tree_hash: String::from("abc123"),
            parent_hashes: vec![String::from("parent-1")],
            message: String::from("initial commit"),
        };
        assert_eq!(p.canonical_bytes(), p.canonical_bytes());
    }

    #[test]
    fn hash_differs_when_message_changes() {
        let mut p = CommitPayload {
            author: String::from("Alice"),
            email: String::from("a@e.com"),
            timestamp_ns: 1,
            tree_hash: String::from("t"),
            parent_hashes: Vec::new(),
            message: String::from("original"),
        };
        let h1 = p.hash();
        p.message = String::from("modified");
        assert_ne!(h1, p.hash());
    }

    #[test]
    fn hash_differs_when_tree_hash_changes() {
        let mut p = CommitPayload {
            author: String::from("Alice"),
            email: String::from("a@e.com"),
            timestamp_ns: 1,
            tree_hash: String::from("t1"),
            parent_hashes: Vec::new(),
            message: String::from("m"),
        };
        let h1 = p.hash();
        p.tree_hash = String::from("t2");
        assert_ne!(h1, p.hash());
    }

    #[test]
    fn parent_list_boundary_disambiguation() {
        let a = CommitPayload {
            author: String::new(),
            email: String::new(),
            timestamp_ns: 0,
            tree_hash: String::new(),
            parent_hashes: vec![String::from("ab"), String::from("cd")],
            message: String::new(),
        };
        let b = CommitPayload {
            author: String::new(),
            email: String::new(),
            timestamp_ns: 0,
            tree_hash: String::new(),
            parent_hashes: vec![String::from("abcd")],
            message: String::new(),
        };
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn signed_commit_verifies() {
        let k = kp(1);
        let payload = CommitPayload {
            author: String::from("A"),
            email: String::from("a@e"),
            timestamp_ns: 1,
            tree_hash: String::from("t"),
            parent_hashes: Vec::new(),
            message: String::from("m"),
        };
        let signed = SignedCommit::sign(&k, payload);
        assert!(signed.verify());
    }

    #[test]
    fn tampered_message_breaks_signature() {
        let k = kp(1);
        let payload = CommitPayload {
            author: String::from("A"),
            email: String::from("a@e"),
            timestamp_ns: 1,
            tree_hash: String::from("t"),
            parent_hashes: Vec::new(),
            message: String::from("original"),
        };
        let mut signed = SignedCommit::sign(&k, payload);
        signed.payload.message = String::from("modified");
        assert!(!signed.verify());
    }

    #[test]
    fn tampered_author_breaks_signature() {
        let k = kp(1);
        let payload = CommitPayload {
            author: String::from("Alice"),
            email: String::from("a@e"),
            timestamp_ns: 1,
            tree_hash: String::from("t"),
            parent_hashes: Vec::new(),
            message: String::from("m"),
        };
        let mut signed = SignedCommit::sign(&k, payload);
        signed.payload.author = String::from("Attacker");
        assert!(!signed.verify());
    }

    #[test]
    fn foreign_signer_is_rejected() {
        let owner = kp(1);
        let attacker = kp(2);
        let payload = CommitPayload {
            author: String::from("Alice"),
            email: String::from("a@e"),
            timestamp_ns: 1,
            tree_hash: String::from("t"),
            parent_hashes: Vec::new(),
            message: String::from("m"),
        };
        let mut signed = SignedCommit::sign(&owner, payload);
        let bytes = signed.payload.canonical_bytes();
        signed.signature = attacker.sign(&bytes);
        assert!(!signed.verify());
    }

    #[test]
    fn empty_chain_tip_hash_is_zero() {
        let chain = CommitChain::new();
        assert_eq!(chain.tip_hash(), 0);
        assert!(chain.is_empty());
    }

    #[test]
    fn root_commit_has_no_parents() {
        let mut chain = CommitChain::new();
        let k = kp(1);
        chain.commit(&k, "Alice", "a@e", 1, "t1", "root");
        assert!(chain.commits()[0].payload.parent_hashes.is_empty());
    }

    #[test]
    fn subsequent_commit_references_parent() {
        let mut chain = CommitChain::new();
        let k = kp(1);
        chain.commit(&k, "Alice", "a@e", 1, "t1", "root");
        let parent_id = format!("{:016x}", chain.tip_hash());
        chain.commit(&k, "Alice", "a@e", 2, "t2", "second");
        assert_eq!(chain.commits()[1].payload.parent_hashes, vec![parent_id]);
    }

    #[test]
    fn intact_chain_is_valid() {
        let mut chain = CommitChain::new();
        let k = kp(1);
        chain.commit(&k, "Alice", "a@e", 1, "t1", "root");
        chain.commit(&k, "Alice", "a@e", 2, "t2", "second");
        chain.commit(&k, "Alice", "a@e", 3, "t3", "third");
        assert!(chain.is_valid());
    }

    #[test]
    fn tampered_commit_breaks_chain() {
        let mut chain = CommitChain::new();
        let k = kp(1);
        chain.commit(&k, "Alice", "a@e", 1, "t1", "root");
        chain.commit(&k, "Alice", "a@e", 2, "t2", "second");
        chain.commits[0].payload.tree_hash = String::from("tampered");
        assert_eq!(chain.find_first_invalid(), Some(0));
    }

    #[test]
    fn authors_lists_distinct() {
        let mut chain = CommitChain::new();
        let k1 = kp(1);
        let k2 = kp(2);
        chain.commit(&k1, "Alice", "a@e", 1, "t", "");
        chain.commit(&k2, "Bob", "b@e", 2, "t", "");
        chain.commit(&k1, "Alice", "a@e", 3, "t", "");
        let authors = chain.authors();
        assert_eq!(authors.len(), 2);
        assert!(authors.contains(&String::from("Alice")));
        assert!(authors.contains(&String::from("Bob")));
    }

    #[test]
    fn commits_by_counts_per_author() {
        let mut chain = CommitChain::new();
        let k1 = kp(1);
        let k2 = kp(2);
        chain.commit(&k1, "Alice", "a@e", 1, "t", "");
        chain.commit(&k2, "Bob", "b@e", 2, "t", "");
        chain.commit(&k1, "Alice", "a@e", 3, "t", "");
        assert_eq!(chain.commits_by("Alice"), 2);
        assert_eq!(chain.commits_by("Bob"), 1);
        assert_eq!(chain.commits_by("Charlie"), 0);
    }
}
