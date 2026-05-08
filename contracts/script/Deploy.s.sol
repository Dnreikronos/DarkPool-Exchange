// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";

contract DeployScript is Script {
    using stdJson for string;

    function run() public {
        address feeRecipient = vm.envAddress("FEE_RECIPIENT");
        uint256 deployerKey = vm.envUint("PRIVATE_KEY");
        string memory vkPath = vm.envString("VK_JSON_PATH");

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

        DarkPool pool = new DarkPool(address(verifier), feeRecipient);
        console.log("DarkPool:", address(pool));

        vm.stopBroadcast();
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
        alpha1 = abi.decode(json.parseRaw(".alpha1"), (uint256[2]));
        beta2 = abi.decode(json.parseRaw(".beta2"), (uint256[2][2]));
        gamma2 = abi.decode(json.parseRaw(".gamma2"), (uint256[2][2]));
        delta2 = abi.decode(json.parseRaw(".delta2"), (uint256[2][2]));
        uint256[2][] memory icDyn = abi.decode(json.parseRaw(".ic"), (uint256[2][]));
        require(icDyn.length == 7, "ic length must be 7");
        for (uint256 i = 0; i < 7; i++) {
            ic[i] = icDyn[i];
        }
    }
}
