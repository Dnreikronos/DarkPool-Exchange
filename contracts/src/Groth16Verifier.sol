// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IVerifier} from "./interfaces/IVerifier.sol";

/// @title Groth16 verifier (BN254) with an immutable verifying key.
/// @notice The verifying key is bound at construction. Upgrades require
///         deploying a new verifier and rewiring downstream contracts.
/// @dev    Deployer-trust assumption: VK points are produced by a trusted
///         setup and serialized in the precompile-canonical (imag, real)
///         order. The constructor only rejects point-at-infinity; off-curve
///         or wrong-subgroup VK points will surface on the first verify call
///         when the precompile rejects them. G2 subgroup self-checks are
///         omitted intentionally (a pairing per coord would balloon deploy
///         gas) — the trusted-setup ceremony is the source of truth.
contract Groth16Verifier is IVerifier {
    uint256 internal constant SNARK_SCALAR_FIELD =
        21888242871839275222246405745257275088548364400416034343698204186575808495617;
    uint256 internal constant PRIME_Q = 21888242871839275222246405745257275088696311157297823662689037894645226208583;

    uint256 internal constant NUM_PUBLIC_INPUTS = 6;
    uint256 internal constant IC_LEN = 7; // NUM_PUBLIC_INPUTS + 1
    uint256 internal constant MAX_BATCH = 256; // mirrors DarkPool.MAX_BATCH_SIZE

    // Fiat-Shamir domain separator. Bumping this string is a hard fork of the
    // batch transcript and is reserved for breaking changes to the batch path.
    bytes32 internal constant FS_DOMAIN = keccak256("Groth16Batch-v1");

    // alpha (G1) — 2 scalars.
    uint256 internal immutable ALPHA1_X;
    uint256 internal immutable ALPHA1_Y;

    // beta (G2) — Fq2 packed [c1, c0] per coordinate to match precompile order.
    uint256 internal immutable BETA2_X1;
    uint256 internal immutable BETA2_X0;
    uint256 internal immutable BETA2_Y1;
    uint256 internal immutable BETA2_Y0;

    // gamma (G2).
    uint256 internal immutable GAMMA2_X1;
    uint256 internal immutable GAMMA2_X0;
    uint256 internal immutable GAMMA2_Y1;
    uint256 internal immutable GAMMA2_Y0;

    // delta (G2).
    uint256 internal immutable DELTA2_X1;
    uint256 internal immutable DELTA2_X0;
    uint256 internal immutable DELTA2_Y1;
    uint256 internal immutable DELTA2_Y0;

    // ic[0..6] (G1) — 14 immutable scalars (Solidity disallows immutable arrays).
    uint256 internal immutable IC_0_X;
    uint256 internal immutable IC_0_Y;
    uint256 internal immutable IC_1_X;
    uint256 internal immutable IC_1_Y;
    uint256 internal immutable IC_2_X;
    uint256 internal immutable IC_2_Y;
    uint256 internal immutable IC_3_X;
    uint256 internal immutable IC_3_Y;
    uint256 internal immutable IC_4_X;
    uint256 internal immutable IC_4_Y;
    uint256 internal immutable IC_5_X;
    uint256 internal immutable IC_5_Y;
    uint256 internal immutable IC_6_X;
    uint256 internal immutable IC_6_Y;

    constructor(
        uint256[2] memory _alpha1,
        uint256[2][2] memory _beta2,
        uint256[2][2] memory _gamma2,
        uint256[2][2] memory _delta2,
        uint256[2][IC_LEN] memory _ic
    ) {
        require(_alpha1[0] != 0 || _alpha1[1] != 0, "alpha1 point at infinity");
        require(_beta2[0][0] != 0 || _beta2[0][1] != 0 || _beta2[1][0] != 0 || _beta2[1][1] != 0, "beta2 point at infinity");
        require(_gamma2[0][0] != 0 || _gamma2[0][1] != 0 || _gamma2[1][0] != 0 || _gamma2[1][1] != 0, "gamma2 point at infinity");
        require(_delta2[0][0] != 0 || _delta2[0][1] != 0 || _delta2[1][0] != 0 || _delta2[1][1] != 0, "delta2 point at infinity");
        for (uint256 i = 0; i < IC_LEN; i++) {
            require(_ic[i][0] != 0 || _ic[i][1] != 0, "ic point at infinity");
        }

        ALPHA1_X = _alpha1[0];
        ALPHA1_Y = _alpha1[1];

        BETA2_X1 = _beta2[0][0];
        BETA2_X0 = _beta2[0][1];
        BETA2_Y1 = _beta2[1][0];
        BETA2_Y0 = _beta2[1][1];

        GAMMA2_X1 = _gamma2[0][0];
        GAMMA2_X0 = _gamma2[0][1];
        GAMMA2_Y1 = _gamma2[1][0];
        GAMMA2_Y0 = _gamma2[1][1];

        DELTA2_X1 = _delta2[0][0];
        DELTA2_X0 = _delta2[0][1];
        DELTA2_Y1 = _delta2[1][0];
        DELTA2_Y0 = _delta2[1][1];

        IC_0_X = _ic[0][0];
        IC_0_Y = _ic[0][1];
        IC_1_X = _ic[1][0];
        IC_1_Y = _ic[1][1];
        IC_2_X = _ic[2][0];
        IC_2_Y = _ic[2][1];
        IC_3_X = _ic[3][0];
        IC_3_Y = _ic[3][1];
        IC_4_X = _ic[4][0];
        IC_4_Y = _ic[4][1];
        IC_5_X = _ic[5][0];
        IC_5_Y = _ic[5][1];
        IC_6_X = _ic[6][0];
        IC_6_Y = _ic[6][1];
    }

    function _ic(uint256 idx) internal view returns (uint256[2] memory) {
        if (idx == 0) return [IC_0_X, IC_0_Y];
        if (idx == 1) return [IC_1_X, IC_1_Y];
        if (idx == 2) return [IC_2_X, IC_2_Y];
        if (idx == 3) return [IC_3_X, IC_3_Y];
        if (idx == 4) return [IC_4_X, IC_4_Y];
        if (idx == 5) return [IC_5_X, IC_5_Y];
        if (idx == 6) return [IC_6_X, IC_6_Y];
        revert("ic out of range");
    }

    /// @notice Verify a single Groth16 proof against the committed VK.
    function verifyProof(
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256[NUM_PUBLIC_INPUTS] calldata input
    ) external view virtual override returns (bool) {
        for (uint256 i = 0; i < NUM_PUBLIC_INPUTS; i++) {
            require(input[i] < SNARK_SCALAR_FIELD, "input overflow");
        }

        uint256[2] memory vkX = _ic(0);
        for (uint256 i = 0; i < NUM_PUBLIC_INPUTS; i++) {
            (uint256 x, uint256 y) = ecMul(_ic(i + 1), input[i]);
            (vkX[0], vkX[1]) = ecAdd(vkX[0], vkX[1], x, y);
        }

        return ecPairingSingle(_negate(a), b, vkX, c);
    }

    /// @notice Batch-verify N Groth16 proofs against the committed VK.
    /// @dev Uses the standard random-linear-combination soundness trick:
    ///      sample r = H(proofs‖inputs); the prover-specific A_i/B_i pairs
    ///      stay as N separate pairings e(r_i · -A_i, B_i) because B_i
    ///      varies per proof, while the constant-side G1 accumulators
    ///        Σ r_i · L_i  (pairs against γ)
    ///        Σ r_i · C_i  (pairs against δ)
    ///        (Σ r_i) · α  (pairs against β)
    ///      collapse into 3 terms. Final pairing has N+3 terms, not 4.
    ///      Any tampered proof is rejected with overwhelming probability;
    ///      a forged sum can't slip past since each B_i is checked separately.
    function verifyProofBatch(
        uint256[2][] calldata aArr,
        uint256[2][2][] calldata bArr,
        uint256[2][] calldata cArr,
        uint256[NUM_PUBLIC_INPUTS][] calldata inputs
    ) external view virtual override returns (bool) {
        uint256 n = aArr.length;
        require(n > 0, "empty batch");
        require(n <= MAX_BATCH, "batch too large");
        require(bArr.length == n && cArr.length == n && inputs.length == n, "length mismatch");
        _checkInputsRange(inputs);

        // Domain-separated FS challenge; rehash on the 1/2^254 chance r0 hits 0
        // (or 1, which would collapse r_i = r0^i to all-ones and degrade RLC to
        // a naive ΣA·B aggregation).
        uint256 r0 = uint256(keccak256(abi.encode(FS_DOMAIN, aArr, bArr, cArr, inputs))) % SNARK_SCALAR_FIELD;
        while (r0 == 0 || r0 == 1) {
            r0 = uint256(keccak256(abi.encode(FS_DOMAIN, r0, "retry"))) % SNARK_SCALAR_FIELD;
        }

        (uint256 rSum, uint256[2] memory accL) = _accumulateRL(inputs, n, r0);

        uint256[2] memory accAlpha;
        (accAlpha[0], accAlpha[1]) = ecMul([ALPHA1_X, ALPHA1_Y], rSum);

        uint256[] memory pIn = _buildBatchPairingInput(aArr, bArr, cArr, n, r0, accAlpha, accL);
        return ecPairingDynamic(pIn);
    }

    function _checkInputsRange(uint256[NUM_PUBLIC_INPUTS][] calldata inputs) internal pure {
        uint256 n = inputs.length;
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < NUM_PUBLIC_INPUTS; j++) {
                require(inputs[i][j] < SNARK_SCALAR_FIELD, "input overflow");
            }
        }
    }

    function _accumulateRL(uint256[NUM_PUBLIC_INPUTS][] calldata inputs, uint256 n, uint256 r0)
        internal
        view
        returns (uint256 rSum, uint256[2] memory accL)
    {
        uint256 ri = 1;
        for (uint256 i = 0; i < n; i++) {
            if (i > 0) ri = mulmod(ri, r0, SNARK_SCALAR_FIELD);
            rSum = addmod(rSum, ri, SNARK_SCALAR_FIELD);

            uint256[2] memory li = _publicInputAccumulator(inputs[i]);
            (uint256 lx, uint256 ly) = ecMul(li, ri);
            (accL[0], accL[1]) = ecAdd(accL[0], accL[1], lx, ly);
        }
    }

    function _publicInputAccumulator(uint256[NUM_PUBLIC_INPUTS] calldata input)
        internal
        view
        returns (uint256[2] memory li)
    {
        li = _ic(0);
        for (uint256 j = 0; j < NUM_PUBLIC_INPUTS; j++) {
            (uint256 x, uint256 y) = ecMul(_ic(j + 1), input[j]);
            (li[0], li[1]) = ecAdd(li[0], li[1], x, y);
        }
    }

    function _buildBatchPairingInput(
        uint256[2][] calldata aArr,
        uint256[2][2][] calldata bArr,
        uint256[2][] calldata cArr,
        uint256 n,
        uint256 r0,
        uint256[2] memory accAlpha,
        uint256[2] memory accL
    ) internal view returns (uint256[] memory pIn) {
        pIn = new uint256[]((n + 3) * 6);

        uint256[2] memory accC;
        uint256 ri = 1;
        for (uint256 i = 0; i < n; i++) {
            if (i > 0) ri = mulmod(ri, r0, SNARK_SCALAR_FIELD);
            (uint256 ax, uint256 ay) = ecMul(_negate(aArr[i]), ri);
            uint256 base = i * 6;
            pIn[base + 0] = ax;
            pIn[base + 1] = ay;
            pIn[base + 2] = bArr[i][0][0];
            pIn[base + 3] = bArr[i][0][1];
            pIn[base + 4] = bArr[i][1][0];
            pIn[base + 5] = bArr[i][1][1];

            (uint256 cx, uint256 cy) = ecMul(cArr[i], ri);
            (accC[0], accC[1]) = ecAdd(accC[0], accC[1], cx, cy);
        }

        uint256 off = n * 6;
        // e(accAlpha, beta).
        pIn[off + 0] = accAlpha[0];
        pIn[off + 1] = accAlpha[1];
        pIn[off + 2] = BETA2_X1;
        pIn[off + 3] = BETA2_X0;
        pIn[off + 4] = BETA2_Y1;
        pIn[off + 5] = BETA2_Y0;

        // e(accL, gamma).
        off += 6;
        pIn[off + 0] = accL[0];
        pIn[off + 1] = accL[1];
        pIn[off + 2] = GAMMA2_X1;
        pIn[off + 3] = GAMMA2_X0;
        pIn[off + 4] = GAMMA2_Y1;
        pIn[off + 5] = GAMMA2_Y0;

        // e(accC, delta).
        off += 6;
        pIn[off + 0] = accC[0];
        pIn[off + 1] = accC[1];
        pIn[off + 2] = DELTA2_X1;
        pIn[off + 3] = DELTA2_X0;
        pIn[off + 4] = DELTA2_Y1;
        pIn[off + 5] = DELTA2_Y0;
    }

    function _negate(uint256[2] calldata p) internal pure returns (uint256[2] memory) {
        if (p[0] == 0 && p[1] == 0) return [uint256(0), 0];
        return [p[0], PRIME_Q - p[1]];
    }

    function ecAdd(uint256 ax, uint256 ay, uint256 bx, uint256 by) internal view returns (uint256, uint256) {
        uint256[4] memory input;
        input[0] = ax;
        input[1] = ay;
        input[2] = bx;
        input[3] = by;
        bool success;
        uint256[2] memory result;
        assembly {
            success := staticcall(gas(), 0x06, input, 0x80, result, 0x40)
        }
        require(success, "ecAdd failed");
        return (result[0], result[1]);
    }

    function ecMul(uint256[2] memory p, uint256 s) internal view returns (uint256, uint256) {
        uint256[3] memory input;
        input[0] = p[0];
        input[1] = p[1];
        input[2] = s;
        bool success;
        uint256[2] memory result;
        assembly {
            success := staticcall(gas(), 0x07, input, 0x60, result, 0x40)
        }
        require(success, "ecMul failed");
        return (result[0], result[1]);
    }

    /// @dev Single-proof pairing: e(-A, B) · e(α, β) · e(L, γ) · e(C, δ) == 1.
    function ecPairingSingle(
        uint256[2] memory a1,
        uint256[2][2] calldata a2,
        uint256[2] memory c1,
        uint256[2] calldata d1
    ) internal view returns (bool) {
        uint256[24] memory input;

        input[0] = a1[0];
        input[1] = a1[1];
        // a2 layout is [[x.c1, x.c0], [y.c1, y.c0]] — feed precompile in
        // canonical (imag, real) order.
        input[2] = a2[0][0];
        input[3] = a2[0][1];
        input[4] = a2[1][0];
        input[5] = a2[1][1];

        input[6] = ALPHA1_X;
        input[7] = ALPHA1_Y;
        input[8] = BETA2_X1;
        input[9] = BETA2_X0;
        input[10] = BETA2_Y1;
        input[11] = BETA2_Y0;

        input[12] = c1[0];
        input[13] = c1[1];
        input[14] = GAMMA2_X1;
        input[15] = GAMMA2_X0;
        input[16] = GAMMA2_Y1;
        input[17] = GAMMA2_Y0;

        input[18] = d1[0];
        input[19] = d1[1];
        input[20] = DELTA2_X1;
        input[21] = DELTA2_X0;
        input[22] = DELTA2_Y1;
        input[23] = DELTA2_Y0;

        uint256[1] memory result;
        bool success;
        assembly {
            success := staticcall(gas(), 0x08, input, 0x300, result, 0x20)
        }
        require(success, "ecPairing failed");
        return result[0] == 1;
    }

    /// @dev Variable-length pairing for the batch path.
    function ecPairingDynamic(uint256[] memory input) internal view returns (bool) {
        uint256[1] memory result;
        bool success;
        uint256 length = input.length * 32;
        assembly {
            success := staticcall(gas(), 0x08, add(input, 0x20), length, result, 0x20)
        }
        require(success, "ecPairing failed");
        return result[0] == 1;
    }
}
