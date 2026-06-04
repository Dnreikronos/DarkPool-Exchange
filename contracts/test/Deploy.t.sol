// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {DeployScript} from "../script/Deploy.s.sol";
import {DarkPool} from "../src/DarkPool.sol";

contract DeployTest is Test {
    using stdJson for string;

    // Non-mainnet id so the deploy writes to a throwaway deployments file
    // we clean up, instead of clobbering the committed 31337.json.
    uint256 constant CHAIN_ID = 424242;
    uint256 constant DEPLOYER_KEY = 0xA11CE;

    address deployer = vm.addr(DEPLOYER_KEY);
    address governor = address(0x60E2);
    address darkPoolOwner = address(0xDA12);

    function setUp() public {
        vm.setEnv("FEE_RECIPIENT", vm.toString(address(0xFEE)));
        vm.setEnv("PRIVATE_KEY", vm.toString(DEPLOYER_KEY));
        vm.setEnv("VK_JSON_PATH", "test/fixtures/vk.json");
        vm.setEnv(
            "OPERATOR_PUBKEY_HEX",
            "0x0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798"
        );
    }

    // The deploy must not leave DarkPool — which escrows every trader's
    // funds — owned by the deploying EOA. On mainnet it refuses the silent
    // fallback; given a DARKPOOL_OWNER it hands the pool over (Ownable2Step,
    // so ownership is pending until the timelock/multisig accepts).
    function test_darkPoolOwnershipHandoff() public {
        // Mainnet without DARKPOOL_OWNER is refused. VERIFIER_GOVERNOR is set
        // so we get past its (earlier) guard and reach the new requirement.
        vm.setEnv("VERIFIER_GOVERNOR", vm.toString(governor));
        vm.chainId(1);
        DeployScript mainnetDeploy = new DeployScript();
        vm.expectRevert(bytes("DARKPOOL_OWNER must be set on mainnet"));
        mainnetDeploy.run();

        // With an owner set, deploy on a non-mainnet chain and confirm the
        // pool is handed to it rather than retained by the deployer.
        vm.setEnv("DARKPOOL_OWNER", vm.toString(darkPoolOwner));
        vm.chainId(CHAIN_ID);
        DeployScript deploy = new DeployScript();
        deploy.run();

        DarkPool pool = DarkPool(_deployedPool());
        assertEq(pool.pendingOwner(), darkPoolOwner, "owner transfer not queued");
        assertEq(pool.owner(), deployer, "owner should change only on acceptOwnership");

        vm.removeFile(_deploymentPath());
    }

    function _deployedPool() internal view returns (address) {
        string memory json = vm.readFile(_deploymentPath());
        return json.readAddress(".darkPool");
    }

    function _deploymentPath() internal view returns (string memory) {
        return string.concat("./deployments/", vm.toString(block.chainid), ".json");
    }
}
