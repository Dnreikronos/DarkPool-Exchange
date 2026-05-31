//! Native fixed-depth Poseidon Merkle tree over admitted-order commitments
//! (input-completeness binding, #157).
//!
//! Each auction round commits to the full multiset of order commitments
//! admitted to that round as a canonical Merkle root. The matching proof
//! proves every settled leg's commitment is a member of that committed set,
//! and the per-round root is folded into the IVC state (`z[4]`). A watcher
//! recomputes the identical root from the public `OrderPlaced` log and checks
//! it against the proof-bound chain, so a semi-trusted operator cannot match
//! an order that was never publicly admitted, nor lie about the admitted input
//! set, without detection.
//!
//! The tree is *canonical*: leaves are sorted ascending by field value and the
//! frontier is padded to `1 << MERKLE_DEPTH` with the empty leaf, so the
//! operator and any independent watcher derive the same root from the same set
//! regardless of insertion order. The node hash is `poseidon(left, right)` via
//! [`hash_two_native`], identical to the in-circuit gadget so engine, prover,
//! and watcher agree byte-for-byte.

use ark_bn254::Fr;
use ark_ff::{PrimeField, Zero};

use crate::pedersen::hash_two_native;
use crate::ZkError;

/// Tree depth. Caps a single round's admitted set at `1 << MERKLE_DEPTH`
/// orders. `2^8 = 256` is ample for the single-pair MVP; raising it widens the
/// in-circuit membership cost (one extra Poseidon hash per matched leg per
/// level) and must be changed in lockstep with the in-circuit gadget in
/// `step_circuit`.
pub const MERKLE_DEPTH: usize = 8;

/// Number of leaves in a full tree.
pub const MERKLE_LEAVES: usize = 1 << MERKLE_DEPTH;

/// The padding leaf for unused slots. Poseidon commitments are ~never zero,
/// and the watcher pads identically, so zero is an unambiguous "empty" marker.
#[inline]
pub fn empty_leaf() -> Fr {
    Fr::zero()
}

/// A membership path: the sibling at each level bottom-up, plus the leaf's
/// position bit at each level (`true` = the current node is the right child,
/// so the sibling sits on the left).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleProof {
    pub siblings: [Fr; MERKLE_DEPTH],
    pub index_bits: [bool; MERKLE_DEPTH],
}

impl Default for MerkleProof {
    fn default() -> Self {
        Self {
            siblings: [Fr::zero(); MERKLE_DEPTH],
            index_bits: [false; MERKLE_DEPTH],
        }
    }
}

/// Canonicalize a set of leaves: sort ascending by field value and pad to a
/// full tree with the empty leaf. Errors if the set exceeds the tree capacity.
fn canonical_leaves(commitments: &[Fr]) -> Result<Vec<Fr>, ZkError> {
    if commitments.len() > MERKLE_LEAVES {
        return Err(ZkError::Witness(format!(
            "admitted set of {} orders exceeds Merkle capacity {MERKLE_LEAVES} (depth {MERKLE_DEPTH})",
            commitments.len()
        )));
    }
    let mut leaves = commitments.to_vec();
    leaves.sort_by_key(|a| a.into_bigint());
    leaves.resize(MERKLE_LEAVES, empty_leaf());
    Ok(leaves)
}

/// Build every tree level bottom-up; `levels[0]` is the padded leaves,
/// `levels[MERKLE_DEPTH]` is `[root]`.
fn build_levels(padded_leaves: Vec<Fr>) -> Vec<Vec<Fr>> {
    let mut levels = Vec::with_capacity(MERKLE_DEPTH + 1);
    levels.push(padded_leaves);
    for d in 0..MERKLE_DEPTH {
        let prev = &levels[d];
        let mut next = Vec::with_capacity(prev.len() / 2);
        for pair in prev.chunks(2) {
            next.push(hash_two_native(pair[0], pair[1]));
        }
        levels.push(next);
    }
    levels
}

/// Compute the canonical admitted-set root over `commitments`.
pub fn admitted_set_root(commitments: &[Fr]) -> Result<Fr, ZkError> {
    let leaves = canonical_leaves(commitments)?;
    let levels = build_levels(leaves);
    Ok(levels[MERKLE_DEPTH][0])
}

/// Compute the root and a membership proof for `leaf` within `commitments`.
/// Returns `Ok(None)` if `leaf` is not present in the set.
pub fn admitted_set_proof(
    commitments: &[Fr],
    leaf: Fr,
) -> Result<Option<(Fr, MerkleProof)>, ZkError> {
    let leaves = canonical_leaves(commitments)?;
    let idx = match leaves.iter().position(|&l| l == leaf) {
        Some(i) => i,
        None => return Ok(None),
    };
    let levels = build_levels(leaves);
    let root = levels[MERKLE_DEPTH][0];

    let mut siblings = [Fr::zero(); MERKLE_DEPTH];
    let mut index_bits = [false; MERKLE_DEPTH];
    let mut node = idx;
    for (d, sibling) in siblings.iter_mut().enumerate() {
        let is_right = node & 1 == 1;
        let sib = if is_right { node - 1 } else { node + 1 };
        *sibling = levels[d][sib];
        index_bits[d] = is_right;
        node /= 2;
    }
    Ok(Some((
        root,
        MerkleProof {
            siblings,
            index_bits,
        },
    )))
}

/// Recompute a root from a leaf and its membership proof — the verifier-side
/// mirror of the in-circuit gadget. A leaf is a member of the tree with root
/// `r` iff `root_from_proof(leaf, proof) == r`.
pub fn root_from_proof(leaf: Fr, proof: &MerkleProof) -> Fr {
    let mut cur = leaf;
    for d in 0..MERKLE_DEPTH {
        cur = if proof.index_bits[d] {
            hash_two_native(proof.siblings[d], cur)
        } else {
            hash_two_native(cur, proof.siblings[d])
        };
    }
    cur
}

/// Fold one round's admitted-set root into the running chain carried in
/// `z[4]`. Mirrors the in-circuit `poseidon(prev, root)`. The empty chain
/// starts at `Fr::zero()` (matching `z_0[4]`), so a watcher reconstructs
/// `z_n[4]` by folding each settled round's recomputed root in order.
pub fn admitted_chain_step(prev: Fr, root: Fr) -> Fr {
    hash_two_native(prev, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(vals: &[u64]) -> Vec<Fr> {
        vals.iter().map(|v| Fr::from(*v)).collect()
    }

    #[test]
    fn root_is_independent_of_insertion_order() {
        let a = admitted_set_root(&leaves(&[3, 1, 2, 9, 7])).unwrap();
        let b = admitted_set_root(&leaves(&[9, 7, 2, 1, 3])).unwrap();
        assert_eq!(a, b, "canonical root must not depend on input order");
    }

    #[test]
    fn membership_proof_recomputes_the_root() {
        let set = leaves(&[10, 20, 30, 40, 50]);
        let (root, _) = admitted_set_proof(&set, Fr::from(30u64))
            .unwrap()
            .expect("30 is a member");
        // The proof for every member must reproduce the single canonical root.
        for v in [10u64, 20, 30, 40, 50] {
            let (r, proof) = admitted_set_proof(&set, Fr::from(v)).unwrap().unwrap();
            assert_eq!(r, root, "all members share one root");
            assert_eq!(
                root_from_proof(Fr::from(v), &proof),
                root,
                "membership path for {v} must recompute the root"
            );
        }
    }

    #[test]
    fn root_matches_standalone_root_helper() {
        let set = leaves(&[5, 6, 7]);
        let (root, _) = admitted_set_proof(&set, Fr::from(6u64)).unwrap().unwrap();
        assert_eq!(root, admitted_set_root(&set).unwrap());
    }

    #[test]
    fn non_member_yields_no_proof() {
        let set = leaves(&[1, 2, 3]);
        assert!(admitted_set_proof(&set, Fr::from(99u64)).unwrap().is_none());
    }

    #[test]
    fn wrong_leaf_does_not_verify_against_a_members_path() {
        let set = leaves(&[1, 2, 3, 4]);
        let (root, proof) = admitted_set_proof(&set, Fr::from(2u64)).unwrap().unwrap();
        // A different leaf threaded through the same path must not hit the root.
        assert_ne!(root_from_proof(Fr::from(7u64), &proof), root);
    }

    #[test]
    fn single_element_set_has_a_valid_path() {
        let set = leaves(&[42]);
        let (root, proof) = admitted_set_proof(&set, Fr::from(42u64)).unwrap().unwrap();
        assert_eq!(root_from_proof(Fr::from(42u64), &proof), root);
    }

    #[test]
    fn capacity_overflow_is_rejected() {
        let set: Vec<Fr> = (0..(MERKLE_LEAVES as u64 + 1)).map(Fr::from).collect();
        assert!(admitted_set_root(&set).is_err());
        assert!(admitted_set_proof(&set, Fr::from(0u64)).is_err());
    }

    #[test]
    fn chain_is_deterministic_and_order_sensitive() {
        let r1 = Fr::from(111u64);
        let r2 = Fr::from(222u64);
        let z0 = Fr::zero();
        let forward = admitted_chain_step(admitted_chain_step(z0, r1), r2);
        let reordered = admitted_chain_step(admitted_chain_step(z0, r2), r1);
        assert_eq!(
            forward,
            admitted_chain_step(admitted_chain_step(z0, r1), r2),
            "chain must be deterministic"
        );
        assert_ne!(forward, reordered, "chain must depend on round order");
    }
}
