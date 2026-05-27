// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";

/// @title Shared Groth16 fixture loader for tests.
/// @dev Centralizes the proof/VK JSON parsing that every verifier-flavored
///      test needs, plus a replicated-batch builder. Inherit and call
///      `_loadFixtureProof()` from setUp to populate the cached proof state.
abstract contract VkFixture is Test {
    using stdJson for string;

    // Cached fixture proof — populated by _loadFixtureProof().
    uint256[2] internal proofA;
    uint256[2][2] internal proofB;
    uint256[2] internal proofC;
    uint256[6] internal proofInputs;

    function _loadFixtureProof() internal {
        _loadProofInto(proofA, proofB, proofC, proofInputs);
    }

    function _loadVk()
        internal
        view
        returns (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        )
    {
        string memory json = vm.readFile("./test/fixtures/vk.json");
        alpha1[0] = json.readUint(".alpha1[0]");
        alpha1[1] = json.readUint(".alpha1[1]");
        _loadG2(json, ".beta2", beta2);
        _loadG2(json, ".gamma2", gamma2);
        _loadG2(json, ".delta2", delta2);
        for (uint256 i = 0; i < 7; i++) {
            ic[i][0] = json.readUint(string.concat(".ic[", vm.toString(i), "][0]"));
            ic[i][1] = json.readUint(string.concat(".ic[", vm.toString(i), "][1]"));
        }
    }

    function _loadG2(string memory json, string memory base, uint256[2][2] memory g) internal pure {
        g[0][0] = json.readUint(string.concat(base, "[0][0]"));
        g[0][1] = json.readUint(string.concat(base, "[0][1]"));
        g[1][0] = json.readUint(string.concat(base, "[1][0]"));
        g[1][1] = json.readUint(string.concat(base, "[1][1]"));
    }

    function _loadProofInto(
        uint256[2] storage a,
        uint256[2][2] storage b,
        uint256[2] storage c,
        uint256[6] storage input
    ) internal {
        string memory json = vm.readFile("./test/fixtures/proof.json");
        a[0] = json.readUint(".a[0]");
        a[1] = json.readUint(".a[1]");
        b[0][0] = json.readUint(".b[0][0]");
        b[0][1] = json.readUint(".b[0][1]");
        b[1][0] = json.readUint(".b[1][0]");
        b[1][1] = json.readUint(".b[1][1]");
        c[0] = json.readUint(".c[0]");
        c[1] = json.readUint(".c[1]");
        for (uint256 i = 0; i < 6; i++) {
            input[i] = json.readUint(string.concat(".public_inputs[", vm.toString(i), "]"));
        }
    }

    /// @dev Build a length-N batch where every entry replays the cached fixture.
    /// Used by the batch tests that don't care about per-proof distinctness.
    function _replicatedBatch(uint256 n)
        internal
        view
        returns (
            uint256[2][] memory aArr,
            uint256[2][2][] memory bArr,
            uint256[2][] memory cArr,
            uint256[6][] memory inputs
        )
    {
        aArr = new uint256[2][](n);
        bArr = new uint256[2][2][](n);
        cArr = new uint256[2][](n);
        inputs = new uint256[6][](n);
        for (uint256 i = 0; i < n; i++) {
            aArr[i] = proofA;
            bArr[i] = proofB;
            cArr[i] = proofC;
            inputs[i] = proofInputs;
        }
    }
}
