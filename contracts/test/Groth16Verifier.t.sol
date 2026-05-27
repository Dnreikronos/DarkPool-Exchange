// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Groth16Verifier} from "../src/Groth16Verifier.sol";
import {VkFixture} from "./VkFixture.sol";

contract Groth16VerifierTest is VkFixture {
    uint256 internal constant SNARK_SCALAR_FIELD =
        21888242871839275222246405745257275088548364400416034343698204186575808495617;

    Groth16Verifier verifier;

    function setUp() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        verifier = new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
        _loadFixtureProof();
    }

    // --- Constructor / immutability ---

    function test_constructor_setsImmutableVk() public view {
        assertTrue(verifier.verifyProof(proofA, proofB, proofC, proofInputs));
    }

    function test_noSetVerifyingKeyFunctionExists() public view {
        // The verifier must not expose any way to mutate the VK. A bare
        // staticcall would also "fail" if the function existed but reverted
        // on missing args, so we scan deployed bytecode for the selector.
        bytes4 legacy =
            bytes4(keccak256("setVerifyingKey(uint256[2],uint256[2][2],uint256[2][2],uint256[2][2],uint256[2][])"));
        bytes memory code = address(verifier).code;
        assertGt(code.length, 0, "verifier has no code");
        bool found = _containsSelector(code, legacy);
        assertFalse(found, "legacy setter selector present in bytecode");
    }

    function _containsSelector(bytes memory code, bytes4 sel) internal pure returns (bool) {
        if (code.length < 4) return false;
        for (uint256 i = 0; i + 4 <= code.length; i++) {
            if (code[i] == sel[0] && code[i + 1] == sel[1] && code[i + 2] == sel[2] && code[i + 3] == sel[3]) {
                return true;
            }
        }
        return false;
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
        (uint256[2][] memory aArr, uint256[2][2][] memory bArr, uint256[2][] memory cArr, uint256[6][] memory inputs) =
            _replicatedBatch(1);
        bool batchOk = verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
        bool singleOk = verifier.verifyProof(proofA, proofB, proofC, proofInputs);
        assertEq(batchOk, singleOk, "batch(1) must match single verify");
    }

    // NOTE: this exercises the batch path with N=3 *identical* fixtures because
    // the fixture generator emits a single proof. RLC soundness is not stressed
    // here — distinct fixtures (different seeds) would be needed to catch a
    // broken cross-term. The poisoned-proof test below covers the rejection
    // branch; tampering tests cover RLC sensitivity in aggregate.
    function test_verifyProofBatch_acceptsValid() public view {
        (uint256[2][] memory aArr, uint256[2][2][] memory bArr, uint256[2][] memory cArr, uint256[6][] memory inputs) =
            _replicatedBatch(3);
        assertTrue(verifier.verifyProofBatch(aArr, bArr, cArr, inputs));
    }

    function test_verifyProofBatch_rejectsAnyInvalid() public {
        (uint256[2][] memory aArr, uint256[2][2][] memory bArr, uint256[2][] memory cArr, uint256[6][] memory inputs) =
            _replicatedBatch(3);
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
        (uint256[2][] memory aArr, uint256[2][2][] memory bArr, uint256[2][] memory cArr, uint256[6][] memory inputs) =
            _replicatedBatch(2);
        inputs[1][3] = type(uint256).max;
        vm.expectRevert("input overflow");
        verifier.verifyProofBatch(aArr, bArr, cArr, inputs);
    }

    // --- Constructor validation ---
    //
    // Each case mutates one VK field to the point-at-infinity and asserts the
    // matching revert. The shared helper _expectInfinityRevert keeps the
    // pattern (load VK → poison field → expect revert → redeploy) in one place.

    function test_constructor_rejectsAlpha1AtInfinity() public {
        _expectInfinityRevert(VkField.Alpha1);
    }

    function test_constructor_rejectsBeta2AtInfinity() public {
        _expectInfinityRevert(VkField.Beta2);
    }

    function test_constructor_rejectsGamma2AtInfinity() public {
        _expectInfinityRevert(VkField.Gamma2);
    }

    function test_constructor_rejectsDelta2AtInfinity() public {
        _expectInfinityRevert(VkField.Delta2);
    }

    function test_constructor_rejectsIcAtInfinity() public {
        _expectInfinityRevert(VkField.Ic);
    }

    enum VkField {
        Alpha1,
        Beta2,
        Gamma2,
        Delta2,
        Ic
    }

    function _expectInfinityRevert(VkField field) internal {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();

        uint256[2] memory zeroG1 = [uint256(0), uint256(0)];
        uint256[2][2] memory zeroG2 = [zeroG1, zeroG1];

        string memory expectedRevert;
        if (field == VkField.Alpha1) {
            alpha1 = zeroG1;
            expectedRevert = "alpha1 point at infinity";
        } else if (field == VkField.Beta2) {
            beta2 = zeroG2;
            expectedRevert = "beta2 point at infinity";
        } else if (field == VkField.Gamma2) {
            gamma2 = zeroG2;
            expectedRevert = "gamma2 point at infinity";
        } else if (field == VkField.Delta2) {
            delta2 = zeroG2;
            expectedRevert = "delta2 point at infinity";
        } else {
            ic[3] = zeroG1;
            expectedRevert = "ic point at infinity";
        }

        vm.expectRevert(bytes(expectedRevert));
        new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
    }

}
