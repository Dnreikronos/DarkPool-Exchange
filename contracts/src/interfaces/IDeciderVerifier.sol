// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title Interface for a HyperNova Decider verifier.
interface IDeciderVerifier {
    /// @notice Verify a HyperNova IVC Decider proof.
    /// @param proof  Serialized Decider proof bytes.
    /// @param z0     IVC initial state (5 BN254 field elements:
    ///        [state_hash, round_nonce, policy_hash, settlement_acc, admit_chain]).
    /// @param zN     IVC final state (same 5-element layout). `zN[3]` is the
    ///        settlement hash-chain DarkPool reproduces from `matches[]` (#209).
    /// @param nSteps Number of folding steps performed.
    function verifyIvcProof(
        bytes calldata proof,
        uint256[5] calldata z0,
        uint256[5] calldata zN,
        uint64 nSteps
    ) external view returns (bool);
}
