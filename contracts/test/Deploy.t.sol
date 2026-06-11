// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {DeployScript} from "../script/Deploy.s.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {HyperNovaDeciderVerifier} from "../src/HyperNovaDeciderVerifier.sol";

contract DeployTest is Test {
    using stdJson for string;

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

    // forge runs a contract's test functions in parallel and vm.setEnv mutates
    // process-global state, so two tests that set IVC_VERIFIER_ADDRESS to
    // different values would race. These tests avoid that: only
    // test_ivc_offAllowlistRequiresRealVerifier touches that env (reading it
    // sequentially within itself), the allowlist tests never read it (the script
    // auto-wires the stub before consulting the env), and every test that writes
    // a deployment file uses a distinct chainid so the deployments/<id>.json
    // paths never collide.

    // The deploy must not leave DarkPool — which escrows every trader's funds —
    // owned by the deploying EOA. On mainnet it refuses the silent fallback;
    // given a DARKPOOL_OWNER it hands the pool over (Ownable2Step, so ownership
    // is pending until the timelock/multisig accepts). Run here on an allowlisted
    // local chain, which also confirms the stub is auto-wired and the IVC path
    // armed for local end-to-end work.
    function test_darkPoolOwnershipHandoff() public {
        // Mainnet without DARKPOOL_OWNER is refused. VERIFIER_GOVERNOR is set
        // so we get past its (earlier) guard and reach the new requirement.
        vm.setEnv("VERIFIER_GOVERNOR", vm.toString(governor));
        vm.chainId(1);
        DeployScript mainnetDeploy = new DeployScript();
        vm.expectRevert(bytes("DARKPOOL_OWNER must be set on mainnet"));
        mainnetDeploy.run();

        // With an owner set, deploy on the allowlisted local devnet (1337, not
        // the committed 31337.json) and confirm the pool is handed over.
        vm.setEnv("DARKPOOL_OWNER", vm.toString(darkPoolOwner));
        vm.chainId(1337);
        DeployScript deploy = new DeployScript();
        deploy.run();

        DarkPool pool = DarkPool(_deployedPool());
        assertEq(pool.pendingOwner(), darkPoolOwner, "owner transfer not queued");
        assertEq(pool.owner(), deployer, "owner should change only on acceptOwnership");
        assertTrue(pool.ivcEnabled(), "local allowlist deploy should arm the IVC stub");

        vm.removeFile(_deploymentPath());
    }

    // Sepolia is on the dev/test allowlist, so the stub is auto-wired and the
    // IVC path armed there too (covers the testnet allowlist member; the local
    // member is covered by test_darkPoolOwnershipHandoff on 1337).
    function test_ivc_stubArmedOnSepolia() public {
        vm.chainId(11155111);
        DeployScript deploy = new DeployScript();
        deploy.run();

        DarkPool pool = DarkPool(_deployedPool());
        assertTrue(pool.ivcEnabled(), "Sepolia deploy should arm the IVC stub");

        vm.removeFile(_deploymentPath());
    }

    // #210, the core guard: off the dev/test allowlist the all-accepting stub is
    // never auto-wired. Without a real IVC_VERIFIER_ADDRESS the deploy reverts so
    // the stub can't reach a real-money chain; with a real, code-bearing decider
    // it wires that verifier but leaves the IVC path DISARMED for the owner to
    // enable only after review. Both halves live in one test so this is the only
    // function that mutates IVC_VERIFIER_ADDRESS (see the parallelism note above).
    function test_ivc_offAllowlistRequiresRealVerifier() public {
        // Base mainnet (8453): real money, not chainid 1. No verifier -> revert.
        vm.setEnv("IVC_VERIFIER_ADDRESS", vm.toString(address(0)));
        vm.chainId(8453);
        DeployScript blocked = new DeployScript();
        vm.expectRevert(
            bytes(
                "IVC_VERIFIER_ADDRESS required off the dev/test allowlist; the stub decider must never settle real funds"
            )
        );
        blocked.run();

        // Optimism mainnet (10): a real, code-bearing decider is supplied. The
        // deploy succeeds and wires it, but leaves the IVC path disarmed.
        address realDecider = address(new HyperNovaDeciderVerifier());
        vm.setEnv("IVC_VERIFIER_ADDRESS", vm.toString(realDecider));
        vm.chainId(10);
        DeployScript deploy = new DeployScript();
        deploy.run();

        DarkPool pool = DarkPool(_deployedPool());
        assertFalse(pool.ivcEnabled(), "real-chain deploy must leave IVC disarmed");
        assertTrue(address(pool.ivcVerifier()) != address(0), "ivc verifier should be wired");

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
