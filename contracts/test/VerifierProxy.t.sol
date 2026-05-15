// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";
import {VerifierProxy} from "../src/VerifierProxy.sol";
import {IVerifier} from "../src/interfaces/IVerifier.sol";
import {VkFixture} from "./VkFixture.sol";

contract VerifierProxyTest is VkFixture {
    uint256 internal constant SNARK_SCALAR_FIELD =
        21888242871839275222246405745257275088548364400416034343698204186575808495617;

    Groth16Verifier backend;
    VerifierProxy proxy;

    address owner = address(this);

    event VerifierUpdated(address indexed oldVerifier, address indexed newVerifier);

    function setUp() public {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        backend = new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
        proxy = new VerifierProxy(address(backend), owner);
        _loadFixtureProof();
    }

    // --- Constructor ---

    function test_constructor_setsBackend() public view {
        assertEq(address(proxy.verifier()), address(backend));
    }

    function test_constructor_setsOwner() public view {
        assertEq(proxy.owner(), owner);
    }

    function test_constructor_rejectsZeroVerifier() public {
        vm.expectRevert("zero verifier");
        new VerifierProxy(address(0), owner);
    }

    function test_constructor_rejectsEoaVerifier() public {
        // An address with no code (EOA, or never-deployed) must fail the
        // code-bearing check rather than silently install a dead pointer.
        vm.expectRevert("verifier has no code");
        new VerifierProxy(address(0xBEEF), owner);
    }

    function test_constructor_rejectsZeroOwner() public {
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableInvalidOwner.selector, address(0)));
        new VerifierProxy(address(backend), address(0));
    }

    function test_constructor_emitsVerifierUpdated() public {
        vm.expectEmit(true, true, false, false);
        emit VerifierUpdated(address(0), address(backend));
        new VerifierProxy(address(backend), owner);
    }

    // --- Forwarding ---

    function test_verifyProof_forwardsToBackend() public view {
        bool viaProxy = proxy.verifyProof(proofA, proofB, proofC, proofInputs);
        bool direct = backend.verifyProof(proofA, proofB, proofC, proofInputs);
        assertTrue(direct, "fixture must verify on the backend");
        assertEq(viaProxy, direct, "proxy result must match backend");
    }

    function test_verifyProof_rejectsTamperedProof() public {
        uint256[2] memory tamperedA = [proofA[0] ^ 1, proofA[1]];
        // Tampered curve coords usually fail the precompile (revert) rather
        // than return false. Either is acceptable proof of rejection.
        try proxy.verifyProof(tamperedA, proofB, proofC, proofInputs) returns (bool ok) {
            assertFalse(ok, "tampered proof must not verify through proxy");
        } catch {
            // precompile rejected — also a valid reject.
        }
    }

    function test_verifyProofBatch_acceptsValid() public view {
        (uint256[2][] memory aArr, uint256[2][2][] memory bArr, uint256[2][] memory cArr, uint256[6][] memory inputs) =
            _replicatedBatch(3);
        assertTrue(proxy.verifyProofBatch(aArr, bArr, cArr, inputs));
    }

    // --- setVerifier ---

    function test_setVerifier_onlyOwner() public {
        address attacker = address(0xdead);
        vm.prank(attacker);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, attacker));
        proxy.setVerifier(address(backend));
    }

    function test_setVerifier_rejectsZero() public {
        vm.expectRevert("zero verifier");
        proxy.setVerifier(address(0));
    }

    function test_setVerifier_rejectsEoa() public {
        vm.expectRevert("verifier has no code");
        proxy.setVerifier(address(0xBEEF));
    }

    function test_setVerifier_rejectsSameVerifier() public {
        // No-op rotation must revert rather than fire a spurious VerifierUpdated
        // event that would fool monitoring of governance rotations.
        vm.expectRevert("same verifier");
        proxy.setVerifier(address(backend));
    }

    function test_setVerifier_swapsBackend() public {
        // Build a second Groth16Verifier with a structurally valid but
        // semantically different VK by swapping two IC points. Both points
        // are still on the curve (they came from the same trusted setup),
        // so the precompile won't reject; the pairing simply won't equal 1
        // for the original proof.
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        uint256[2] memory tmp = ic[1];
        ic[1] = ic[2];
        ic[2] = tmp;
        Groth16Verifier altered = new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);

        assertTrue(proxy.verifyProof(proofA, proofB, proofC, proofInputs), "sanity: proof verifies pre-swap");

        proxy.setVerifier(address(altered));
        assertEq(address(proxy.verifier()), address(altered));

        try proxy.verifyProof(proofA, proofB, proofC, proofInputs) returns (bool ok) {
            assertFalse(ok, "original proof must not verify under swapped VK");
        } catch {
            // pairing precompile rejected — also a valid reject.
        }
    }

    function test_setVerifier_emitsVerifierUpdated() public {
        Groth16Verifier next = _deployFreshBackend();
        vm.expectEmit(true, true, false, false);
        emit VerifierUpdated(address(backend), address(next));
        proxy.setVerifier(address(next));
    }

    // --- Ownership (2-step) ---

    function test_ownership_twoStep() public {
        address newOwner = address(0x777);

        proxy.transferOwnership(newOwner);
        assertEq(proxy.owner(), owner, "owner unchanged before accept");
        assertEq(proxy.pendingOwner(), newOwner);

        vm.prank(newOwner);
        proxy.acceptOwnership();
        assertEq(proxy.owner(), newOwner);
        assertEq(proxy.pendingOwner(), address(0));

        // New owner can rotate; old owner is locked out.
        Groth16Verifier next = _deployFreshBackend();
        vm.prank(newOwner);
        proxy.setVerifier(address(next));
        assertEq(address(proxy.verifier()), address(next));

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, owner));
        proxy.setVerifier(address(backend));
    }

    // --- Gas budget ---

    function test_verifyProofViaProxy_overheadBounded() public view {
        // Measure the proxy hop as a delta over a direct backend call rather
        // than an absolute budget. The direct path is already covered by
        // Groth16Verifier.t.sol's <300k assertion; here we just want to bound
        // the additional cost of one typed external call + storage read.
        //
        // Warm both contract addresses first — EIP-2929 charges 2600 gas the
        // first time an account is touched in a tx and 100 gas thereafter, so
        // an unwarmed direct call would look ~2500 gas more expensive than a
        // proxy call that piggybacks on the proxy's own warm-state.
        backend.verifyProof(proofA, proofB, proofC, proofInputs);
        proxy.verifyProof(proofA, proofB, proofC, proofInputs);

        uint256 t0 = gasleft();
        bool okDirect = backend.verifyProof(proofA, proofB, proofC, proofInputs);
        uint256 directUsed = t0 - gasleft();

        uint256 t1 = gasleft();
        bool okProxy = proxy.verifyProof(proofA, proofB, proofC, proofInputs);
        uint256 proxyUsed = t1 - gasleft();

        assertTrue(okDirect, "direct call must verify");
        assertTrue(okProxy, "proxy call must verify");
        assertGe(proxyUsed, directUsed, "proxy should not be cheaper than direct");
        assertLt(proxyUsed - directUsed, 10_000, "proxy overhead exceeded 10k gas");
    }

    // --- Fuzz ---

    function testFuzz_verifyProofViaProxy_inputOverflow(uint256 idx, uint256 val) public {
        idx = idx % 6;
        vm.assume(val >= SNARK_SCALAR_FIELD);
        uint256[6] memory bad = proofInputs;
        bad[idx] = val;
        // Overflow check fires in the backend; the proxy bubbles the revert
        // (Solidity propagates the revert reason from typed external calls).
        vm.expectRevert("input overflow");
        proxy.verifyProof(proofA, proofB, proofC, bad);
    }

    function testFuzz_setVerifier_anyNonOwnerReverts(address caller) public {
        vm.assume(caller != owner);
        vm.assume(caller != address(0));
        vm.prank(caller);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, caller));
        proxy.setVerifier(address(backend));
    }

    // --- Helpers ---

    function _deployFreshBackend() internal returns (Groth16Verifier) {
        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk();
        return new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
    }

}
