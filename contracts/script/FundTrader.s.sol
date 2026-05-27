// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {MockERC20} from "../test/mocks/MockERC20.sol";

/// @notice Deploys mock WETH + USDC tokens and funds a test trader address.
/// Usage: TRADER=0x... forge script script/FundTrader.s.sol:FundTrader --rpc-url ... --broadcast
contract FundTrader is Script {
    function run() public {
        uint256 deployerKey = vm.envUint("PRIVATE_KEY");
        address trader = vm.envOr("TRADER", vm.addr(deployerKey));

        vm.startBroadcast(deployerKey);

        MockERC20 weth = new MockERC20("Wrapped Ether", "WETH", 18);
        MockERC20 usdc = new MockERC20("USD Coin", "USDC", 6);

        weth.mint(trader, 100 ether);
        usdc.mint(trader, 1_000_000e6);

        console.log("WETH:", address(weth));
        console.log("USDC:", address(usdc));
        console.log("Trader:", trader);
        console.log("WETH balance:", weth.balanceOf(trader));
        console.log("USDC balance:", usdc.balanceOf(trader));

        vm.stopBroadcast();
    }
}
