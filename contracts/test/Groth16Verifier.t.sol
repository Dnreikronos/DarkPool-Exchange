// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";

contract Groth16VerifierTest is Test {
    using stdJson for string;

    uint256 internal constant SNARK_SCALAR_FIELD =
        21888242871839275222246405745257275088548364400416034343698204186575808495617;

    Groth16Verifier verifier;

    // Cached fixture proof for reuse across tests.
    uint256[2] proofA;
    uint256[2][2] proofB;
    uint256[2] proofC;
    uint256[6] proofInputs;

    function setUp() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        verifier = new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
        _loadProofInto(proofA, proofB, proofC, proofInputs);
    }

    // --- Constructor / immutability ---

    function test_constructor_setsImmutableVk() public view {
        assertTrue(verifier.verifyProof(proofA, proofB, proofC, proofInputs));
    }

    function test_noSetVerifyingKeyFunctionExists() public {
        // The verifier must not expose any way to mutate the VK.
        // Confirm the legacy selector is not present on the deployed bytecode.
        bytes4 legacy =
            bytes4(keccak256("setVerifyingKey(uint256[2],uint256[2][2],uint256[2][2],uint256[2][2],uint256[2][])"));
        (bool ok,) = address(verifier).staticcall(abi.encodeWithSelector(legacy));
        assertFalse(ok, "legacy setter still callable");
    }

    // --- Single proof ---

    function test_verifyProof_acceptsKnownGood() public view {
        bool ok = verifier.verifyProof(proofA, proofB, proofC, proofInputs);
        assertTrue(ok);
    }

    function test_verifyProof_rejectsTamperedProof() public {
        uint256[2] memory tamperedA = [proofA[0] ^ 1, proofA[1]];
        // Tampered curve coords usually fail the precompile (revert) rather
        // than return false. Either is acceptable proof of rejection.
        try verifier.verifyProof(tamperedA, proofB, proofC, proofInputs) returns (bool ok) {
            assertFalse(ok, "tampered proof must not verify");
        } catch {
            // Precompile rejected the malformed point — also a valid reject.
        }
    }

    function test_verifyProof_rejectsTamperedInput() public view {
        uint256[6] memory bad = proofInputs;
        bad[1] ^= 1;
        bool ok = verifier.verifyProof(proofA, proofB, proofC, bad);
        assertFalse(ok);
    }

    function test_verifyProof_inputOverflow_reverts() public {
        uint256[6] memory bad = proofInputs;
        bad[0] = type(uint256).max;
        vm.expectRevert("input overflow");
        verifier.verifyProof(proofA, proofB, proofC, bad);
    }

    // --- Gas budget ---

    function test_verifyProof_gasUnder300k() public view {
        uint256 startGas = gasleft();
        bool ok = verifier.verifyProof(proofA, proofB, proofC, proofInputs);
        uint256 used = startGas - gasleft();
        assertTrue(ok);
        assertLt(used, 300_000, "verifyProof exceeded 300k gas budget");
    }

    // --- Fuzz ---

    function testFuzz_verifyProof_inputOverflow(uint256 idx, uint256 val) public {
        idx = idx % 6;
        vm.assume(val >= SNARK_SCALAR_FIELD);
        uint256[6] memory bad = proofInputs;
        bad[idx] = val;
        vm.expectRevert("input overflow");
        verifier.verifyProof(proofA, proofB, proofC, bad);
    }

    function testFuzz_verifyProof_rejectsRandomGarbage(uint256 a0, uint256 a1, uint256 c0, uint256 c1, uint256 i0)
        public
    {
        // Bound public inputs into the scalar field; the rest is unconstrained.
        i0 = i0 % SNARK_SCALAR_FIELD;
        uint256[2] memory a = [a0, a1];
        uint256[2] memory c = [c0, c1];
        uint256[6] memory inp = proofInputs;
        inp[0] = i0;

        // Random points are overwhelmingly off-curve, so the precompile
        // either rejects (revert) or the pairing != 1 (false).
        try verifier.verifyProof(a, proofB, c, inp) returns (bool ok) {
            assertFalse(ok, "random garbage proof must not verify");
        } catch {
            // precompile rejected — also a valid reject.
        }
    }

    // --- Batch verification ---

    function test_verifyProofBatch_singleMatchesSingle() public view {
        uint256[2][] memory aArr = new uint256[2][](1);
        uint256[2][2][] memory bArr = new uint256[2][2][](1);
        uint256[2][] memory cArr = new uint256[2][](1);
        uint256[6][] memory inputs = new uint256[6][](1);
        aArr[0] = proofA;
        bArr[0] = proofB;
        cArr[0] = proofC;
        inputs[0] = proofInputs;
        bool batchOk = verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
        bool singleOk = verifier.verifyProof(proofA, proofB, proofC, proofInputs);
        assertEq(batchOk, singleOk, "batch(1) must match single verify");
    }

    function test_verifyProofBatch_acceptsValid() public view {
        uint256 n = 3;
        uint256[2][] memory aArr = new uint256[2][](n);
        uint256[2][2][] memory bArr = new uint256[2][2][](n);
        uint256[2][] memory cArr = new uint256[2][](n);
        uint256[6][] memory inputs = new uint256[6][](n);
        for (uint256 i = 0; i < n; i++) {
            aArr[i] = proofA;
            bArr[i] = proofB;
            cArr[i] = proofC;
            inputs[i] = proofInputs;
        }
        assertTrue(verifier.verifyProofBatch(aArr, bArr, cArr, inputs));
    }

    function test_verifyProofBatch_rejectsAnyInvalid() public {
        uint256 n = 3;
        uint256[2][] memory aArr = new uint256[2][](n);
        uint256[2][2][] memory bArr = new uint256[2][2][](n);
        uint256[2][] memory cArr = new uint256[2][](n);
        uint256[6][] memory inputs = new uint256[6][](n);
        for (uint256 i = 0; i < n; i++) {
            aArr[i] = proofA;
            bArr[i] = proofB;
            cArr[i] = proofC;
            inputs[i] = proofInputs;
        }
        // Poison the middle proof's public input.
        inputs[1][1] ^= 1;
        try verifier.verifyProofBatch(aArr, bArr, cArr, inputs) returns (bool ok) {
            assertFalse(ok, "poisoned batch must not verify");
        } catch {
            // pairing precompile may revert on degenerate aggregate point;
            // also a valid reject.
        }
    }

    function test_verifyProofBatch_lengthMismatch_reverts() public {
        uint256[2][] memory aArr = new uint256[2][](2);
        uint256[2][2][] memory bArr = new uint256[2][2][](1);
        uint256[2][] memory cArr = new uint256[2][](2);
        uint256[6][] memory inputs = new uint256[6][](2);
        vm.expectRevert("length mismatch");
        verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
    }

    function test_verifyProofBatch_empty_reverts() public {
        uint256[2][] memory aArr = new uint256[2][](0);
        uint256[2][2][] memory bArr = new uint256[2][2][](0);
        uint256[2][] memory cArr = new uint256[2][](0);
        uint256[6][] memory inputs = new uint256[6][](0);
        vm.expectRevert("empty batch");
        verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
    }

    function test_verifyProofBatch_oversize_reverts() public {
        uint256 n = 257; // one over MAX_BATCH
        uint256[2][] memory aArr = new uint256[2][](n);
        uint256[2][2][] memory bArr = new uint256[2][2][](n);
        uint256[2][] memory cArr = new uint256[2][](n);
        uint256[6][] memory inputs = new uint256[6][](n);
        vm.expectRevert("batch too large");
        verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
    }

    function test_verifyProofBatch_inputOverflow_reverts() public {
        uint256 n = 2;
        uint256[2][] memory aArr = new uint256[2][](n);
        uint256[2][2][] memory bArr = new uint256[2][2][](n);
        uint256[2][] memory cArr = new uint256[2][](n);
        uint256[6][] memory inputs = new uint256[6][](n);
        for (uint256 i = 0; i < n; i++) {
            aArr[i] = proofA;
            bArr[i] = proofB;
            cArr[i] = proofC;
            inputs[i] = proofInputs;
        }
        inputs[1][3] = type(uint256).max;
        vm.expectRevert("input overflow");
        verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
    }

    // --- Constructor validation ---

    function test_constructor_rejectsAlpha1AtInfinity() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        alpha1 = [uint256(0), uint256(0)];
        vm.expectRevert("alpha1 point at infinity");
        new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
    }

    function test_constructor_rejectsBeta2AtInfinity() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        beta2 = [[uint256(0), uint256(0)], [uint256(0), uint256(0)]];
        vm.expectRevert("beta2 point at infinity");
        new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
    }

    function test_constructor_rejectsGamma2AtInfinity() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        gamma2 = [[uint256(0), uint256(0)], [uint256(0), uint256(0)]];
        vm.expectRevert("gamma2 point at infinity");
        new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
    }

    function test_constructor_rejectsDelta2AtInfinity() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        delta2 = [[uint256(0), uint256(0)], [uint256(0), uint256(0)]];
        vm.expectRevert("delta2 point at infinity");
        new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
    }

    function test_constructor_rejectsIcAtInfinity() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        ic[3] = [uint256(0), uint256(0)];
        vm.expectRevert("ic point at infinity");
        new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
    }

    // --- Fixture loaders ---

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
}
