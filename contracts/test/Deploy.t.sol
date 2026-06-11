// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {DeployScript} from "../script/Deploy.s.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {HyperNovaDeciderVerifier} from "../src/HyperNovaDeciderVerifier.sol";

// Stand-in for a real Decider verifier in deploy tests: code-bearing and, unlike
// HyperNovaDeciderVerifier, not all-accepting (distinct runtime bytecode, hence a
// distinct codehash), so it passes the deploy's stub-rejection guard.
contract NotTheStubDecider {
    function verifyIvcProof(bytes calldata, uint256[5] calldata, uint256[5] calldata, uint64)
        external
        pure
        returns (bool)
    {
        return false;
    }
}

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
    // process-global state, so two tests that set the same governance env var
    // (VERIFIER_GOVERNOR, DARKPOOL_OWNER, IVC_VERIFIER_ADDRESS) to different
    // values — or one needing it set while another needs it absent — would race.
    // test_deployGuardsAndOwnershipHandoff is the sole reader/writer of those
    // three vars and walks every case sequentially within itself. The Sepolia
    // test touches none of them (dev/test chain: governor/owner fall back and
    // the stub is auto-wired) and asserts only ivcEnabled, so it stays
    // standalone and race-free. Each test that writes a deployment file uses a
    // distinct chainid so the deployments/<id>.json paths never collide.

    // Sepolia is on the dev/test allowlist, so the stub is auto-wired and the
    // IVC path armed. Asserts only ivcEnabled, which is independent of the
    // governance env vars the comprehensive test mutates.
    function test_ivc_stubArmedOnSepolia() public {
        vm.chainId(11155111);
        DeployScript deploy = new DeployScript();
        deploy.run();

        DarkPool pool = DarkPool(_deployedPool());
        assertTrue(pool.ivcEnabled(), "Sepolia deploy should arm the IVC stub");

        vm.removeFile(_deploymentPath());
    }

    // #210 and its adjacent governance finding. On every chain off the dev/test
    // allowlist — not just Ethereum L1 — the deploy must refuse to silently fall
    // back to the deploying EOA for the verifier governor and pool owner, and
    // must refuse to wire the all-accepting stub decider. Walked here in one
    // function (the sole toucher of the three governance env vars, so it cannot
    // race a parallel test). One DeployScript instance is reused so expectRevert
    // only ever wraps run(), never a contract creation.
    function test_deployGuardsAndOwnershipHandoff() public {
        DeployScript deploy = new DeployScript();

        // 1. Dev chain (legacy local 1337, not the committed 31337.json): no
        // governance env set, so governor and owner fall back to the deployer
        // (no transfer) and the stub is auto-wired and armed.
        vm.chainId(1337);
        deploy.run();
        DarkPool devPool = DarkPool(_deployedPool());
        assertEq(devPool.owner(), deployer, "dev chain should keep the deployer as owner");
        assertEq(devPool.pendingOwner(), address(0), "dev chain should not queue a transfer");
        assertTrue(devPool.ivcEnabled(), "dev chain should arm the IVC stub");
        vm.removeFile(_deploymentPath());

        // 2. Real chain (Base mainnet 8453) with no VERIFIER_GOVERNOR -> reject.
        vm.chainId(8453);
        vm.expectRevert(bytes("VERIFIER_GOVERNOR must be set on non-dev/test chains"));
        deploy.run();

        // 3. Governor set, but no DARKPOOL_OWNER -> reject.
        vm.setEnv("VERIFIER_GOVERNOR", vm.toString(governor));
        vm.expectRevert(bytes("DARKPOOL_OWNER must be set on non-dev/test chains"));
        deploy.run();

        // 4. Governor + owner set, but no real IVC verifier -> reject (the stub
        // must never reach a real-money chain).
        vm.setEnv("DARKPOOL_OWNER", vm.toString(darkPoolOwner));
        vm.expectRevert(
            bytes(
                "IVC_VERIFIER_ADDRESS required off the dev/test allowlist; the stub decider must never settle real funds"
            )
        );
        deploy.run();

        // 4b. Even a code-bearing address is rejected if it is the all-accepting
        // stub itself: requiring an explicit address must not become a backdoor
        // for wiring the very verifier this guard keeps off real chains.
        vm.setEnv("IVC_VERIFIER_ADDRESS", vm.toString(address(new HyperNovaDeciderVerifier())));
        vm.expectRevert(bytes("IVC_VERIFIER_ADDRESS must not be the all-accepting stub decider"));
        deploy.run();

        // 5. A real (non-stub) decider plus governor and owner: the deploy
        // succeeds, queues the ownership handoff (Ownable2Step, pending until
        // acceptOwnership), and leaves the IVC path disarmed for owner review.
        vm.setEnv("IVC_VERIFIER_ADDRESS", vm.toString(address(new NotTheStubDecider())));
        deploy.run();
        DarkPool pool = DarkPool(_deployedPool());
        assertEq(pool.pendingOwner(), darkPoolOwner, "owner transfer not queued");
        assertEq(pool.owner(), deployer, "owner should change only on acceptOwnership");
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
