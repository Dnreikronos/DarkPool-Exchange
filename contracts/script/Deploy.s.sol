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
        // and CI deploys. Off the dev/test allowlist — i.e. on every real
        // chain, not just Ethereum L1 — we refuse the silent fallback: a single
        // EOA holding VK-rotation rights would be a governance footgun.
        require(
            _isDevTestChain(block.chainid) || vm.envExists("VERIFIER_GOVERNOR"),
            "VERIFIER_GOVERNOR must be set on non-dev/test chains"
        );
        address governor = vm.envOr("VERIFIER_GOVERNOR", deployer);

        // DARKPOOL_OWNER: address that controls operator management,
        // policy, fee recipient, operator-pubkey rotation, the IVC
        // verifier pointer, and pause on the contract that escrows every
        // trader's funds. In production this must be a TimelockController
        // (or a multisig fronting one); the default falls back to the
        // deployer for local and CI deploys. Off the dev/test allowlist — i.e.
        // on every real chain, not just Ethereum L1 — we refuse the silent
        // fallback: leaving full protocol control on a single deploying EOA is
        // the centralisation risk flagged in #166.
        require(
            _isDevTestChain(block.chainid) || vm.envExists("DARKPOOL_OWNER"),
            "DARKPOOL_OWNER must be set on non-dev/test chains"
        );
        address darkPoolOwner = vm.envOr("DARKPOOL_OWNER", deployer);

        // IVC verifier backend selection (#210). The HyperNova decider is still
        // a stub that accepts every proof, so it must never be wired into this
        // fund-custody contract on a real chain. Dev/test chains (local +
        // Sepolia) auto-wire the stub and arm the IVC path for end-to-end
        // testing; every other chain - including every L2/L1 mainnet - MUST
        // supply a real, audited decider via IVC_VERIFIER_ADDRESS, validated
        // HERE before vm.startBroadcast so a missing/invalid verifier aborts
        // with nothing deployed (the same fail-fast contract as the env checks
        // above) rather than reverting mid-broadcast.
        bool devTestChain = _isDevTestChain(block.chainid);
        address realIvcVerifier;
        if (!devTestChain) {
            // address(0) sentinel: an unset OR explicitly-zero env var both mean
            // "no real verifier provided", which is the reject case.
            realIvcVerifier = vm.envOr("IVC_VERIFIER_ADDRESS", address(0));
            require(
                realIvcVerifier != address(0),
                "IVC_VERIFIER_ADDRESS required off the dev/test allowlist; the stub decider must never settle real funds"
            );
            require(realIvcVerifier.code.length > 0, "IVC_VERIFIER_ADDRESS has no code");
            // Requiring an explicit address must not become a backdoor for
            // wiring the very verifier this guard exists to keep off real
            // chains: the all-accepting stub has code, so the length check alone
            // lets it through. Reject it by runtime-bytecode hash. This catches
            // the stub compiled from this repo; a real decider is a distinct
            // contract with a distinct codehash.
            require(
                realIvcVerifier.codehash != keccak256(type(HyperNovaDeciderVerifier).runtimeCode),
                "IVC_VERIFIER_ADDRESS must not be the all-accepting stub decider"
            );
        }

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

        // Wire the IVC verifier decided above. On dev/test chains deploy the
        // stub now and arm the path; off-allowlist use the pre-validated real
        // verifier and leave the path DISARMED until the owner reviews it.
        address ivcImpl;
        if (devTestChain) {
            HyperNovaDeciderVerifier hypernova = new HyperNovaDeciderVerifier();
            ivcImpl = address(hypernova);
            console.log("HyperNovaDeciderVerifier (STUB - dev/test only):", ivcImpl);
        } else {
            ivcImpl = realIvcVerifier;
            console.log("IVC verifier (real, from IVC_VERIFIER_ADDRESS):", ivcImpl);
        }
        // Route IVC verification through the proxy so key rotation only requires
        // proxy.setIvcVerifier(newImpl) - no DarkPool redeployment.
        proxy.setIvcVerifier(ivcImpl);
        pool.setIvcVerifier(address(proxy));
        if (devTestChain) {
            pool.setIvcEnabled(true);
            console.log("IVC settlement path armed (dev/test stub)");
        } else {
            console.log("IVC path DISARMED - owner must call setIvcEnabled(true) after review");
        }

        if (governor != deployer) {
            proxy.transferOwnership(governor);
            console.log("VerifierProxy ownership transferred to governor:", governor);
        }

        // Hand the pool to its production owner last, after every
        // deployer-as-owner setup call above (setTokenAllowed,
        // setIvcVerifier). DarkPool is Ownable2Step, so this only queues
        // the transfer — darkPoolOwner must call acceptOwnership() to take
        // control, which keeps a fat-fingered address from bricking
        // governance.
        if (darkPoolOwner != deployer) {
            pool.transferOwnership(darkPoolOwner);
            console.log("DarkPool ownership transfer initiated (awaiting acceptOwnership):", darkPoolOwner);
        }

        // Records the concrete decider behind the proxy (stub on dev/test, the
        // real verifier off-allowlist) under the unchanged hypernovaDeciderVerifier
        // JSON key (no rename) so existing deployment-file consumers keep working.
        _writeDeployment(
            address(pool),
            address(proxy),
            address(verifier),
            ivcImpl
        );

        vm.stopBroadcast();
    }

    /// @dev Dev/test chains, where production hardening is deliberately relaxed:
    ///      local dev (Anvil 31337, legacy 1337) and the Sepolia testnet
    ///      (11155111). On these the deploy may fall back to the deployer EOA
    ///      for the verifier governor and the pool owner, and may auto-wire and
    ///      arm the all-accepting stub decider. Every other chain - including
    ///      every L2/L1 mainnet - must instead supply an explicit
    ///      VERIFIER_GOVERNOR, DARKPOOL_OWNER, and a real decider via
    ///      IVC_VERIFIER_ADDRESS (#210).
    function _isDevTestChain(uint256 chainId) internal pure returns (bool) {
        return chainId == 31337 || chainId == 1337 || chainId == 11155111;
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
