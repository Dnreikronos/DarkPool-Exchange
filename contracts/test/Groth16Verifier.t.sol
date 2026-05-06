// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";

contract Groth16VerifierTest is Test {
    Groth16Verifier verifier;

    function setUp() public {
        verifier = new Groth16Verifier();
    }

    function test_ownerIsDeployer() public view {
        assertEq(verifier.owner(), address(this));
    }

    function test_vkNotInitialized() public view {
        assertFalse(verifier.vkInitialized());
    }

    function test_setVerifyingKey_wrongIcLength_reverts() public {
        uint256[2] memory alpha1;
        uint256[2][2] memory beta2;
        uint256[2][2] memory gamma2;
        uint256[2][2] memory delta2;
        uint256[2][] memory ic = new uint256[2][](3);

        vm.expectRevert("ic length must be 7");
        verifier.setVerifyingKey(alpha1, beta2, gamma2, delta2, ic);
    }

    function test_setVerifyingKey_success() public {
        _setDummyVk();
        assertTrue(verifier.vkInitialized());
    }

    function test_verifyProof_reverts_when_vk_not_set() public {
        Groth16Verifier fresh = new Groth16Verifier();
        uint256[2] memory a;
        uint256[2][2] memory b;
        uint256[2] memory c;
        uint256[6] memory input;

        vm.expectRevert("vk not set");
        fresh.verifyProof(a, b, c, input);
    }

    function test_setVerifyingKey_onlyOwner() public {
        uint256[2] memory alpha1;
        uint256[2][2] memory beta2;
        uint256[2][2] memory gamma2;
        uint256[2][2] memory delta2;
        uint256[2][] memory ic = new uint256[2][](7);

        vm.prank(address(0xdead));
        vm.expectRevert("not owner");
        verifier.setVerifyingKey(alpha1, beta2, gamma2, delta2, ic);
    }

    function test_verifyProof_inputOverflow_reverts() public {
        _setDummyVk();

        uint256[2] memory a;
        uint256[2][2] memory b;
        uint256[2] memory c;
        uint256[6] memory input;
        input[0] = type(uint256).max;

        vm.expectRevert("input overflow");
        verifier.verifyProof(a, b, c, input);
    }

    function _setDummyVk() internal {
        uint256[2] memory alpha1;
        uint256[2][2] memory beta2;
        uint256[2][2] memory gamma2;
        uint256[2][2] memory delta2;
        uint256[2][] memory ic = new uint256[2][](7);

        alpha1[0] = 1;
        alpha1[1] = 2;

        verifier.setVerifyingKey(alpha1, beta2, gamma2, delta2, ic);
    }
}
