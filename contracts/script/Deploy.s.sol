// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {Groth16Verifier} from "../src/Groth16Verifier.sol";

contract DeployScript is Script {
    function run() public {
        address feeRecipient = vm.envAddress("FEE_RECIPIENT");
        uint256 deployerKey = vm.envUint("PRIVATE_KEY");

        vm.startBroadcast(deployerKey);

        Groth16Verifier verifier = new Groth16Verifier();
        console.log("Groth16Verifier:", address(verifier));

        DarkPool pool = new DarkPool(address(verifier), feeRecipient);
        console.log("DarkPool:", address(pool));

        vm.stopBroadcast();
    }
}
