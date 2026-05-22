// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title Interface for a HyperNova Decider verifier.
interface IDeciderVerifier {
    /// @notice Verify a HyperNova IVC Decider proof.
    /// @param proof  Serialized Decider proof bytes.
    /// @param z0     IVC initial state (3 BN254 field elements).
    /// @param zN     IVC final state (3 BN254 field elements).
    /// @param nSteps Number of folding steps performed.
    function verifyIvcProof(
        bytes calldata proof,
        uint256[3] calldata z0,
        uint256[3] calldata zN,
        uint64 nSteps
    ) external view returns (bool);
}
