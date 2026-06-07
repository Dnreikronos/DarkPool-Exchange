// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice ERC20 that burns a fixed bps fee on every transfer, so the
///         recipient receives less than the requested amount. Used to
///         verify DarkPool.deposit credits the measured balance delta
///         rather than the requested amount.
contract FeeOnTransferERC20 is ERC20 {
    uint256 public immutable feeBps;
    uint256 private constant BPS_DENOMINATOR = 10_000;

    constructor(string memory name_, string memory symbol_, uint256 feeBps_) ERC20(name_, symbol_) {
        require(feeBps_ < BPS_DENOMINATOR, "fee too high");
        feeBps = feeBps_;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    /// @dev Deducts `feeBps` of every transfer by burning it, so the
    ///      recipient is credited `amount - fee`. Overrides the internal
    ///      `_update` hook used by both transfer and transferFrom.
    function _update(address from, address to, uint256 value) internal override {
        // Mint/burn (from or to == address(0)) pass through untouched.
        if (from == address(0) || to == address(0)) {
            super._update(from, to, value);
            return;
        }
        uint256 fee = value * feeBps / BPS_DENOMINATOR;
        if (fee > 0) {
            super._update(from, address(0), fee); // burn the fee
        }
        super._update(from, to, value - fee);
    }
}
