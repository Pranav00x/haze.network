//! A standard binary Merkle tree over sha2::Sha256, used to commit an
//! off-chain allowlist (see core::allowlist) to a single 32-byte root stored
//! on-chain in core::collections::MintPhase, so a mint can prove membership
//! without putting the whole list on-chain. Pure [u8;32] in/out, no
//! dependency on any chain/crypto type - trivially testable in isolation.
//!
//! Convention (must match bit-for-bit on the JS/WASM side that builds
//! proofs client-side): at each level, pairs are combined left-to-right in
//! array order; an odd node out at a level is paired with itself (duplicate-
//! last-leaf padding) rather than promoted unpaired. A proof is the list of
//! sibling hashes from the leaf's level up to the root, and `leaf_index`
//! (the leaf's position at the bottom level) determines, at each level,
//! whether the current node is the left or right child (index's bit, LSB
//! first) - this determines hash(current, sibling) vs hash(sibling, current)
//! ordering during verification.

use sha2::{Sha256, Digest};

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// The root of an empty tree - a well-defined sentinel distinct from any
/// real hash-pair output (a bare SHA256 of nothing), so "no allowlist"
/// (empty leaf set) is never confusable with a legitimate single/multi-leaf
/// root.
pub fn empty_root() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"HazeMerkleEmpty");
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Computes the root of a binary Merkle tree over `leaves`, in the order
/// given (callers needing determinism regardless of input order should sort
/// leaves themselves before calling - core::allowlist does this).
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return empty_root();
    }
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            next.push(hash_pair(&left, &right));
            i += 2;
        }
        level = next;
    }
    level[0]
}

/// Builds the sibling-hash proof path for `leaves[leaf_index]`, from the
/// leaf's own level up to (but not including) the root.
pub fn build_merkle_proof(leaves: &[[u8; 32]], leaf_index: usize) -> Vec<[u8; 32]> {
    let mut proof = Vec::new();
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    let mut index = leaf_index;
    while level.len() > 1 {
        let pair_index = index ^ 1; // sibling: index-1 if index is odd, index+1 if even
        let sibling = if pair_index < level.len() { level[pair_index] } else { level[index] };
        proof.push(sibling);

        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            next.push(hash_pair(&left, &right));
            i += 2;
        }
        level = next;
        index /= 2;
    }
    proof
}

/// Verifies that `leaf` at position `leaf_index` is included under `root`,
/// given a sibling-hash `proof` path (as produced by build_merkle_proof).
pub fn verify_merkle_proof(leaf: [u8; 32], proof: &[[u8; 32]], leaf_index: usize, root: [u8; 32]) -> bool {
    let mut current = leaf;
    let mut index = leaf_index;
    for sibling in proof {
        current = if index % 2 == 0 { hash_pair(&current, sibling) } else { hash_pair(sibling, &current) };
        index /= 2;
    }
    current == root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(n: u8) -> [u8; 32] {
        let mut l = [0u8; 32];
        l[0] = n;
        l
    }

    #[test]
    fn empty_tree_root_is_the_sentinel() {
        assert_eq!(merkle_root(&[]), empty_root());
    }

    #[test]
    fn single_leaf_root_is_deterministic_and_distinct_from_empty() {
        let leaves = vec![leaf(1)];
        let root = merkle_root(&leaves);
        assert_ne!(root, empty_root());
        assert_eq!(root, merkle_root(&leaves));
    }

    #[test]
    fn valid_proof_is_accepted_for_every_leaf_at_various_sizes() {
        for n in [1usize, 2, 3, 4, 5, 7, 8] {
            let leaves: Vec<[u8; 32]> = (0..n as u8).map(leaf).collect();
            let root = merkle_root(&leaves);
            for i in 0..n {
                let proof = build_merkle_proof(&leaves, i);
                assert!(verify_merkle_proof(leaves[i], &proof, i, root), "leaf {} of {} failed", i, n);
            }
        }
    }

    #[test]
    fn tampered_leaf_is_rejected() {
        let leaves = vec![leaf(1), leaf(2), leaf(3)];
        let root = merkle_root(&leaves);
        let proof = build_merkle_proof(&leaves, 1);
        assert!(!verify_merkle_proof(leaf(99), &proof, 1, root));
    }

    #[test]
    fn tampered_proof_sibling_is_rejected() {
        let leaves = vec![leaf(1), leaf(2), leaf(3), leaf(4)];
        let root = merkle_root(&leaves);
        let mut proof = build_merkle_proof(&leaves, 2);
        proof[0] = leaf(200);
        assert!(!verify_merkle_proof(leaves[2], &proof, 2, root));
    }

    #[test]
    fn wrong_leaf_index_is_rejected() {
        let leaves = vec![leaf(1), leaf(2), leaf(3), leaf(4)];
        let root = merkle_root(&leaves);
        let proof = build_merkle_proof(&leaves, 2);
        assert!(!verify_merkle_proof(leaves[2], &proof, 1, root));
    }

    #[test]
    fn wrong_root_is_rejected() {
        let leaves = vec![leaf(1), leaf(2)];
        let other_leaves = vec![leaf(9), leaf(10)];
        let wrong_root = merkle_root(&other_leaves);
        let proof = build_merkle_proof(&leaves, 0);
        assert!(!verify_merkle_proof(leaves[0], &proof, 0, wrong_root));
    }

    #[test]
    fn roots_differ_for_different_leaf_sets() {
        let a = merkle_root(&[leaf(1), leaf(2)]);
        let b = merkle_root(&[leaf(1), leaf(3)]);
        assert_ne!(a, b);
    }
}
