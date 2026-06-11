// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {PoseidonConstants} from "./PoseidonConstants.sol";

/// @title PoseidonBN254
/// @notice Faithful on-chain port of the arkworks `PoseidonSponge` the DarkPool
///         ZK circuit uses, so `DarkPool` can recompute the settlement
///         hash-chain over `matches[]` and bind it to the proof (#209).
///
///         Configuration (must match `dp_zk::pedersen::poseidon_config`):
///         BN254 Fr, state width t = 3 (rate 2 + capacity 1), 8 full + 57
///         partial rounds, x^5 S-box, constants from
///         `find_poseidon_ark_and_mds(254, 2, 8, 57, 0)`.
///
///         Sponge semantics mirror arkworks exactly: the state is
///         capacity-first (`state[0]` is the capacity element), absorbed
///         elements are *added* into the rate slots `state[1..3]`, each round
///         applies `add ark → S-box → MDS`, full rounds S-box all three
///         elements while partial rounds S-box only `state[0]`, and a squeeze
///         returns `state[1]`. Cross-checked against Rust reference vectors in
///         `contracts/test/PoseidonBN254.t.sol`.
library PoseidonBN254 {
    uint256 internal constant F = PoseidonConstants.FIELD_MODULUS;

    uint256 private constant FULL_ROUNDS = 8;
    uint256 private constant PARTIAL_ROUNDS = 57;
    uint256 private constant TOTAL_ROUNDS = FULL_ROUNDS + PARTIAL_ROUNDS; // 65
    uint256 private constant HALF_FULL = FULL_ROUNDS / 2; // 4

    /// x^5 mod F.
    function _sbox(uint256 x) private pure returns (uint256) {
        uint256 x2 = mulmod(x, x, F);
        uint256 x4 = mulmod(x2, x2, F);
        return mulmod(x4, x, F);
    }

    /// One Poseidon permutation of the 3-element state. Rounds `[0, HALF_FULL)`
    /// and `[TOTAL_ROUNDS - HALF_FULL, TOTAL_ROUNDS)` are full (S-box on all
    /// three elements); the middle `PARTIAL_ROUNDS` are partial (S-box on the
    /// capacity element `state[0]` only). `ark`/`mds` are passed in so the
    /// caller loads the constants once per hash rather than once per round.
    function _permute(uint256[3][65] memory ark, uint256[3][3] memory mds, uint256 s0, uint256 s1, uint256 s2)
        private
        pure
        returns (uint256, uint256, uint256)
    {
        for (uint256 r = 0; r < TOTAL_ROUNDS; r++) {
            // add round constants
            s0 = addmod(s0, ark[r][0], F);
            s1 = addmod(s1, ark[r][1], F);
            s2 = addmod(s2, ark[r][2], F);

            // S-box: full rounds on all elements, partial rounds on state[0]
            if (r < HALF_FULL || r >= TOTAL_ROUNDS - HALF_FULL) {
                s0 = _sbox(s0);
                s1 = _sbox(s1);
                s2 = _sbox(s2);
            } else {
                s0 = _sbox(s0);
            }

            // MDS multiply: new[i] = sum_j mds[i][j] * state[j]
            uint256 n0 = addmod(addmod(mulmod(mds[0][0], s0, F), mulmod(mds[0][1], s1, F), F), mulmod(mds[0][2], s2, F), F);
            uint256 n1 = addmod(addmod(mulmod(mds[1][0], s0, F), mulmod(mds[1][1], s1, F), F), mulmod(mds[1][2], s2, F), F);
            uint256 n2 = addmod(addmod(mulmod(mds[2][0], s0, F), mulmod(mds[2][1], s1, F), F), mulmod(mds[2][2], s2, F), F);
            s0 = n0;
            s1 = n1;
            s2 = n2;
        }
        return (s0, s1, s2);
    }

    /// `poseidon([a, b, c, d, e])`: absorb five field elements at rate 2 (three
    /// permutations: blocks `[a,b]`, `[c,d]`, `[e]`) and squeeze one element.
    /// All inputs MUST already be reduced mod F (callers pass addresses and
    /// 1e8-scaled amounts, all < F).
    function poseidon5(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e) internal pure returns (uint256) {
        uint256[3][65] memory ark = PoseidonConstants.ark();
        uint256[3][3] memory mds = PoseidonConstants.mds();

        // state = [capacity, rate0, rate1]
        uint256 s0 = 0;
        uint256 s1 = 0;
        uint256 s2 = 0;

        // absorb [a, b], permute
        s1 = addmod(s1, a, F);
        s2 = addmod(s2, b, F);
        (s0, s1, s2) = _permute(ark, mds, s0, s1, s2);

        // absorb [c, d], permute
        s1 = addmod(s1, c, F);
        s2 = addmod(s2, d, F);
        (s0, s1, s2) = _permute(ark, mds, s0, s1, s2);

        // absorb [e] (partial block), permute
        s1 = addmod(s1, e, F);
        (s0, s1, s2) = _permute(ark, mds, s0, s1, s2);

        // squeeze the first rate element
        return s1;
    }
}
