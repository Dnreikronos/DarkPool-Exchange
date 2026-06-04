// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

/// @notice Malicious ERC20 whose transferFrom returns true but moves no
///         tokens. Used to verify DarkPool.deposit rejects a deposit that
///         credits nothing ("no tokens received") rather than emitting a
///         phantom balance. Mirrors a 100% fee-on-transfer / broken token.
contract NoopTransferERC20 is IERC20 {
    string public name = "Noop";
    string public symbol = "NOOP";
    uint8 public decimals = 18;

    mapping(address => uint256) private _balances;
    mapping(address => mapping(address => uint256)) private _allowances;
    uint256 private _totalSupply;

    function mint(address to, uint256 amount) external {
        _balances[to] += amount;
        _totalSupply += amount;
    }

    function totalSupply() external view returns (uint256) {
        return _totalSupply;
    }

    function balanceOf(address account) external view returns (uint256) {
        return _balances[account];
    }

    function allowance(address owner, address spender) external view returns (uint256) {
        return _allowances[owner][spender];
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        _allowances[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transfer(address, uint256) external pure returns (bool) {
        // Moves nothing, reports success.
        return true;
    }

    /// @dev Reports success but transfers zero tokens — the pool's measured
    ///      balance delta is therefore 0.
    function transferFrom(address, address, uint256) external pure returns (bool) {
        return true;
    }
}
