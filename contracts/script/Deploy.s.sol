// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";
import {VerifierProxy} from "../src/VerifierProxy.sol";
import {HyperNovaDeciderVerifier} from "../src/HyperNovaDeciderVerifier.sol";

contract DeployScript is Script {
    using stdJson for string;

    function run() public {
        address feeRecipient = vm.envAddress("FEE_RECIPIENT");
        uint256 deployerKey = vm.envUint("PRIVATE_KEY");
        string memory vkPath = vm.envString("VK_JSON_PATH");
        // SEC1-encoded operator ECIES pubkey (33-byte compressed or
        // 65-byte uncompressed). Required at deploy: the DarkPool
        // constructor refuses to initialise without one so clients can
        // discover the encryption key from chain state alone.
        bytes memory operatorPubkey = vm.parseBytes(vm.envString("OPERATOR_PUBKEY_HEX"));
        // Fail before vm.startBroadcast() so a malformed env var doesn't
        // leave Groth16Verifier + VerifierProxy deployed but DarkPool
        // missing — those partial deploys waste gas and force a manual
        // address-bookkeeping fix.
        require(
            operatorPubkey.length == 33 || operatorPubkey.length == 65,
            "OPERATOR_PUBKEY_HEX must be 33 (compressed) or 65 (uncompressed) bytes"
        );
        bytes1 pubkeyTag = operatorPubkey[0];
        require(
            (operatorPubkey.length == 33 && (pubkeyTag == 0x02 || pubkeyTag == 0x03))
                || (operatorPubkey.length == 65 && pubkeyTag == 0x04),
            "OPERATOR_PUBKEY_HEX SEC1 tag does not match length"
        );
        address deployer = vm.addr(deployerKey);
        // VERIFIER_GOVERNOR: address that can rotate the verifier backend.
        // In production this should be a TimelockController (or a multisig
        // fronting one); the default falls back to the deployer for local
        // and CI deploys. On mainnet we refuse the silent fallback — a single
        // EOA holding VK-rotation rights would be a governance footgun.
        require(
            block.chainid != 1 || vm.envExists("VERIFIER_GOVERNOR"),
            "VERIFIER_GOVERNOR must be set on mainnet"
        );
        address governor = vm.envOr("VERIFIER_GOVERNOR", deployer);

        (
            uint256[2] memory alpha1,
            uint256[2][2] memory beta2,
            uint256[2][2] memory gamma2,
            uint256[2][2] memory delta2,
            uint256[2][7] memory ic
        ) = _loadVk(vkPath);

        vm.startBroadcast(deployerKey);

        Groth16Verifier verifier = new Groth16Verifier(alpha1, beta2, gamma2, delta2, ic);
        console.log("Groth16Verifier:", address(verifier));

        VerifierProxy proxy = new VerifierProxy(address(verifier), deployer);
        console.log("VerifierProxy:", address(proxy));
        console.log("VerifierProxy owner (interim):", deployer);

        DarkPool pool = new DarkPool(address(proxy), feeRecipient, operatorPubkey);
        console.log("DarkPool:", address(pool));
        console.log("OperatorPubkey bytes:", operatorPubkey.length);

        // Allowlist the tradeable tokens so the pool accepts deposits out of
        // the box. deposit() rejects any token not on the allowlist, so a
        // pool deployed without this can never take a deposit. Pass a
        // comma-free single address per env var; both are optional so local
        // smoke deploys can skip them and allowlist later via setTokenAllowed.
        if (vm.envExists("BASE_TOKEN")) {
            address baseToken = vm.envAddress("BASE_TOKEN");
            pool.setTokenAllowed(baseToken, true);
            console.log("Allowlisted base token:", baseToken);
        }
        if (vm.envExists("QUOTE_TOKEN")) {
            address quoteToken = vm.envAddress("QUOTE_TOKEN");
            pool.setTokenAllowed(quoteToken, true);
            console.log("Allowlisted quote token:", quoteToken);
        }

        require(
            block.chainid != 1,
            "stub IVC verifier cannot be deployed on mainnet; use a real Decider verifier"
        );
        HyperNovaDeciderVerifier hypernova = new HyperNovaDeciderVerifier();
        console.log("HyperNovaDeciderVerifier:", address(hypernova));
        // Route IVC verification through the proxy so key rotation only
        // requires proxy.setIvcVerifier(newImpl) — no DarkPool redeployment.
        proxy.setIvcVerifier(address(hypernova));
        pool.setIvcVerifier(address(proxy));
        console.log("IVC verifier set on DarkPool via VerifierProxy (stub)");

        if (governor != deployer) {
            proxy.transferOwnership(governor);
            console.log("VerifierProxy ownership transferred to governor:", governor);
        }

        _writeDeployment(
            address(pool),
            address(proxy),
            address(verifier),
            address(hypernova)
        );

        vm.stopBroadcast();
    }

    function _writeDeployment(
        address darkPool,
        address verifierProxy,
        address groth16Verifier,
        address hypernovaVerifier
    ) internal {
        string memory out = "deployment";
        out.serialize("darkPool", darkPool);
        out.serialize("verifierProxy", verifierProxy);
        out.serialize("groth16Verifier", groth16Verifier);
        out.serialize("hypernovaDeciderVerifier", hypernovaVerifier);
        out.serialize("deployedAt", block.timestamp);
        out.serialize("blockNumber", block.number);
        string memory gitSha = vm.envOr("GIT_SHA", string("unknown"));
        string memory json = out.serialize("gitSha", gitSha);

        string memory path = string.concat(
            "./deployments/", vm.toString(block.chainid), ".json"
        );
        vm.writeJson(json, path);
        console.log("Deployment written to:", path);
    }

    function _loadVk(string memory path)
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
        string memory json = vm.readFile(path);
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
}
