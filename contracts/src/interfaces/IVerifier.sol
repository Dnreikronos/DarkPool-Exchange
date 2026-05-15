// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title Groth16 verifier surface used by DarkPool and the governance proxy.
/// @dev The 6-element public-input shape is part of the interface. A circuit
///      revision that changes the public-input count is a hard fork — the
///      governance proxy's contract-address swap cannot paper over it.
interface IVerifier {
    function verifyProof(
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256[6] calldata input
    ) external view returns (bool);

    function verifyProofBatch(
        uint256[2][] calldata aArr,
        uint256[2][2][] calldata bArr,
        uint256[2][] calldata cArr,
        uint256[6][] calldata inputs
    ) external view returns (bool);
}
