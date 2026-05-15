// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {IVerifier} from "./interfaces/IVerifier.sol";

/// @title Governance-swappable router in front of an immutable Groth16Verifier.
/// @notice DarkPool talks to this proxy via a fixed address; the proxy holds a
///         mutable pointer to a concrete IVerifier implementation. Rotating the
///         verifying key is "deploy a fresh Groth16Verifier(newVk) → call
///         setVerifier(newAddr)" — DarkPool itself never redeploys.
/// @dev    This is a typed router, NOT an EIP-1967 delegatecall proxy. Each
///         backend is a frozen, independently auditable artifact tied to one
///         VK; governance moves the pointer, not the bytes. The proxy is
///         intentionally minimal: no fallback, no storage writes during
///         forwarding, no timelock baked in. Compose timelocks externally via
///         Ownable2Step.transferOwnership(timelock).
contract VerifierProxy is IVerifier, Ownable2Step {
    IVerifier public verifier;

    event VerifierUpdated(address indexed oldVerifier, address indexed newVerifier);

    constructor(address initialVerifier, address initialOwner) Ownable(initialOwner) {
        _setVerifier(initialVerifier, address(0));
    }

    /// @notice Point the proxy at a new IVerifier implementation.
    /// @dev Code-bearing check catches EOAs and self-destructed contracts, but
    ///      cannot validate that `newVerifier` actually implements IVerifier
    ///      or carries the intended VK — that's a governance review concern.
    function setVerifier(address newVerifier) external onlyOwner {
        address old = address(verifier);
        require(newVerifier != old, "same verifier");
        _setVerifier(newVerifier, old);
    }

    function _setVerifier(address newVerifier, address old) internal {
        require(newVerifier != address(0), "zero verifier");
        // Self-assignment would make verifyProof() recurse into the proxy until
        // OOG. Recoverable via another rotation, but a clear governance footgun
        // — reject it up front.
        require(newVerifier != address(this), "self verifier");
        require(newVerifier.code.length > 0, "verifier has no code");
        verifier = IVerifier(newVerifier);
        emit VerifierUpdated(old, newVerifier);
    }

    function verifyProof(
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256[6] calldata input
    ) external view override returns (bool) {
        return verifier.verifyProof(a, b, c, input);
    }

    function verifyProofBatch(
        uint256[2][] calldata aArr,
        uint256[2][2][] calldata bArr,
        uint256[2][] calldata cArr,
        uint256[6][] calldata inputs
    ) external view override returns (bool) {
        return verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
    }
}
