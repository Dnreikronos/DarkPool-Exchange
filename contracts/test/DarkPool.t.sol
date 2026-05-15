// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {IDarkPool} from "../src/interfaces/IDarkPool.sol";
import {IVerifier} from "../src/interfaces/IVerifier.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {MockERC20} from "./mocks/MockERC20.sol";

contract MockVerifier is IVerifier {
    bool public shouldReturn;

    constructor(bool shouldReturn_) {
        shouldReturn = shouldReturn_;
    }

    function verifyProof(uint256[2] calldata, uint256[2][2] calldata, uint256[2] calldata, uint256[6] calldata)
        external
        view
        returns (bool)
    {
        return shouldReturn;
    }

    function verifyProofBatch(
        uint256[2][] calldata,
        uint256[2][2][] calldata,
        uint256[2][] calldata,
        uint256[6][] calldata
    ) external view returns (bool) {
        return shouldReturn;
    }
}

contract DarkPoolTest is Test {
    DarkPool pool;
    MockVerifier verifier;
    MockERC20 baseToken;
    MockERC20 quoteToken;

    address owner = address(this);
    address operator = address(0x1);
    address feeRecipient = address(0x2);
    address trader1 = address(0x10);
    address trader2 = address(0x20);

    function setUp() public {
        verifier = new MockVerifier(true);
        pool = new DarkPool(address(verifier), feeRecipient);
        pool.addOperator(operator);

        baseToken = new MockERC20("Base", "BASE", 18);
        quoteToken = new MockERC20("Quote", "QUOTE", 18);
    }

    // --- Deposit ---

    function test_deposit() public {
        uint256 amount = 100e18;
        quoteToken.mint(trader1, amount);

        vm.startPrank(trader1);
        quoteToken.approve(address(pool), amount);
        pool.deposit(address(quoteToken), amount);
        vm.stopPrank();

        assertEq(pool.balances(trader1, address(quoteToken)), amount);
        assertEq(quoteToken.balanceOf(address(pool)), amount);
    }

    function test_deposit_zero_reverts() public {
        vm.expectRevert("zero amount");
        pool.deposit(address(quoteToken), 0);
    }

    function test_deposit_emits_event() public {
        uint256 amount = 50e18;
        quoteToken.mint(trader1, amount);

        vm.startPrank(trader1);
        quoteToken.approve(address(pool), amount);

        vm.expectEmit(true, true, false, true);
        emit IDarkPool.Deposit(trader1, address(quoteToken), amount);
        pool.deposit(address(quoteToken), amount);
        vm.stopPrank();
    }

    // --- Withdraw ---

    function test_withdraw() public {
        uint256 amount = 100e18;
        _deposit(trader1, address(quoteToken), amount);

        vm.prank(trader1);
        pool.withdraw(address(quoteToken), amount);

        assertEq(pool.balances(trader1, address(quoteToken)), 0);
        assertEq(quoteToken.balanceOf(trader1), amount);
    }

    function test_withdraw_zero_reverts() public {
        vm.prank(trader1);
        vm.expectRevert("zero amount");
        pool.withdraw(address(quoteToken), 0);
    }

    function test_withdraw_insufficient_reverts() public {
        vm.prank(trader1);
        vm.expectRevert("insufficient balance");
        pool.withdraw(address(quoteToken), 1);
    }

    // --- Operator auth ---

    function test_addOperator() public {
        address newOp = address(0x99);
        pool.addOperator(newOp);
        assertTrue(pool.operators(newOp));
    }

    function test_addOperator_notOwner_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.addOperator(address(0x99));
    }

    function test_removeOperator() public {
        pool.removeOperator(operator);
        assertFalse(pool.operators(operator));
    }

    function test_addOperator_zeroAddress_reverts() public {
        vm.expectRevert("zero address");
        pool.addOperator(address(0));
    }

    // --- submitBatch ---

    function test_submitBatch_notOperator_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert("not operator");
        pool.submitBatch(bytes32(0), bytes32(0), new bytes(256), _zeroInputs(), new IDarkPool.Match[](1));
    }

    function test_submitBatch_alreadySettled_reverts() public {
        bytes32 batchId = bytes32(uint256(1));
        _fundAndSubmitBatch(batchId);

        vm.prank(operator);
        vm.expectRevert("already settled");
        pool.submitBatch(batchId, bytes32(0), new bytes(256), _zeroInputs(), _singleMatch());
    }

    function test_submitBatch_emptyBatch_reverts() public {
        vm.prank(operator);
        vm.expectRevert("invalid batch size");
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), _zeroInputs(), new IDarkPool.Match[](0));
    }

    function test_submitBatch_invalidProofLength_reverts() public {
        uint256[6] memory inputs;
        inputs[0] = 1;
        IDarkPool.Match[] memory matches = _singleMatch();

        vm.prank(operator);
        vm.expectRevert("invalid proof length");
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(128), inputs, matches);
    }

    function test_submitBatch_matchCountMismatch_reverts() public {
        uint256[6] memory inputs;
        inputs[0] = 999;

        vm.prank(operator);
        vm.expectRevert("match count mismatch");
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), inputs, _singleMatch());
    }

    function test_submitBatch_invalidProof_reverts() public {
        MockVerifier badVerifier = new MockVerifier(false);
        DarkPool badPool = new DarkPool(address(badVerifier), feeRecipient);
        badPool.addOperator(operator);

        uint256[6] memory inputs;
        inputs[0] = 1;
        IDarkPool.Match[] memory matches = _singleMatch();

        vm.prank(operator);
        vm.expectRevert("invalid proof");
        badPool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), inputs, matches);
    }

    function test_submitBatch_settlesBalances() public {
        bytes32 batchId = bytes32(uint256(42));
        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;
        uint256 fee = notional * 5 / 10_000;

        _deposit(trader1, address(quoteToken), notional);
        _deposit(trader2, address(baseToken), size);

        uint256[6] memory inputs;
        inputs[0] = 1;

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(1)),
            askOrderId: bytes32(uint256(2)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: price,
            size: size
        });

        vm.prank(operator);
        pool.submitBatch(batchId, bytes32(0), new bytes(256), inputs, matches);

        assertTrue(pool.settled(batchId));
        assertEq(pool.balances(trader1, address(quoteToken)), 0);
        assertEq(pool.balances(trader1, address(baseToken)), size);
        assertEq(pool.balances(trader2, address(baseToken)), 0);
        assertEq(pool.balances(trader2, address(quoteToken)), notional - fee);
        assertEq(pool.balances(feeRecipient, address(quoteToken)), fee);
    }

    function test_submitBatch_emitsEvent() public {
        bytes32 batchId = bytes32(uint256(77));
        _setupBatchBalances();

        uint256[6] memory inputs;
        inputs[0] = 1;

        vm.prank(operator);
        vm.expectEmit(true, false, false, true);
        emit IDarkPool.BatchSettled(batchId, block.timestamp);
        pool.submitBatch(batchId, bytes32(0), new bytes(256), inputs, _fundedMatch());
    }

    // --- Fee math ---

    function test_fee_calculation() public {
        uint256 price = 200e18;
        uint256 size = 5e18;
        uint256 notional = price * size / 1e18;
        uint256 expectedFee = notional * 5 / 10_000;

        _deposit(trader1, address(quoteToken), notional);
        _deposit(trader2, address(baseToken), size);

        uint256[6] memory inputs;
        inputs[0] = 1;

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(1)),
            askOrderId: bytes32(uint256(2)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: price,
            size: size
        });

        vm.prank(operator);
        pool.submitBatch(bytes32(uint256(99)), bytes32(0), new bytes(256), inputs, matches);

        assertEq(pool.balances(feeRecipient, address(quoteToken)), expectedFee);
        assertEq(expectedFee, 5e17);
    }

    // --- Policy ---

    function test_setPolicy() public {
        pool.setPolicy(1e18, 50e18, 1000e18);
        assertEq(pool.minSize(), 1e18);
        assertEq(pool.minPrice(), 50e18);
        assertEq(pool.positionLimit(), 1000e18);
    }

    function test_setPolicy_notOwner_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.setPolicy(0, 0, 0);
    }

    function test_submitBatch_policyMismatch_reverts() public {
        pool.setPolicy(1e18, 0, 0);

        uint256[6] memory inputs;
        inputs[0] = 1;
        inputs[3] = 0;

        vm.prank(operator);
        vm.expectRevert("minSize mismatch");
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), inputs, _singleMatch());
    }

    // --- Pause ---

    function test_pause_blocksDeposit() public {
        pool.pause();
        vm.expectRevert(abi.encodeWithSignature("EnforcedPause()"));
        pool.deposit(address(quoteToken), 1);
    }

    function test_pause_blocksWithdraw() public {
        pool.pause();
        vm.prank(trader1);
        vm.expectRevert(abi.encodeWithSignature("EnforcedPause()"));
        pool.withdraw(address(quoteToken), 1);
    }

    function test_unpause() public {
        pool.pause();
        pool.unpause();

        uint256 amount = 10e18;
        quoteToken.mint(trader1, amount);
        vm.startPrank(trader1);
        quoteToken.approve(address(pool), amount);
        pool.deposit(address(quoteToken), amount);
        vm.stopPrank();

        assertEq(pool.balances(trader1, address(quoteToken)), amount);
    }

    // --- setFeeRecipient ---

    function test_setFeeRecipient() public {
        address newRecipient = address(0x55);
        pool.setFeeRecipient(newRecipient);
        assertEq(pool.feeRecipient(), newRecipient);
    }

    function test_setFeeRecipient_zero_reverts() public {
        vm.expectRevert("zero address");
        pool.setFeeRecipient(address(0));
    }

    // --- Constructor ---

    function test_constructor_zeroVerifier_reverts() public {
        vm.expectRevert("zero verifier");
        new DarkPool(address(0), feeRecipient);
    }

    function test_constructor_zeroFeeRecipient_reverts() public {
        vm.expectRevert("zero fee recipient");
        new DarkPool(address(verifier), address(0));
    }

    // --- Immutable verifier ---

    function test_verifier_isImmutable() public view {
        assertEq(address(pool.verifier()), address(verifier));
    }

    // --- transferOwnership (2-step) ---

    function test_transferOwnership_twoStep() public {
        address newOwner = address(0x777);

        pool.transferOwnership(newOwner);
        assertEq(pool.owner(), owner);
        assertEq(pool.pendingOwner(), newOwner);

        vm.prank(newOwner);
        pool.acceptOwnership();
        assertEq(pool.owner(), newOwner);
        assertEq(pool.pendingOwner(), address(0));

        vm.prank(newOwner);
        pool.addOperator(address(0x888));
        assertTrue(pool.operators(address(0x888)));

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        pool.addOperator(address(0x999));
    }

    function test_transferOwnership_notOwner_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.transferOwnership(address(0x777));
    }

    function test_acceptOwnership_notPending_reverts() public {
        pool.transferOwnership(address(0x777));

        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.acceptOwnership();
    }

    // --- removeOperator auth ---

    function test_removeOperator_notOwner_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.removeOperator(operator);
    }

    // --- pause auth ---

    function test_pause_notOwner_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.pause();
    }

    function test_unpause_notOwner_reverts() public {
        pool.pause();
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.unpause();
    }

    function test_submitBatch_whenPaused_reverts() public {
        pool.pause();
        vm.prank(operator);
        vm.expectRevert(abi.encodeWithSignature("EnforcedPause()"));
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), _zeroInputs(), _singleMatch());
    }

    // --- Policy mismatch branches ---

    function test_submitBatch_minPriceMismatch_reverts() public {
        pool.setPolicy(0, 1e18, 0);

        uint256[6] memory inputs;
        inputs[0] = 1;
        inputs[4] = 0;

        vm.prank(operator);
        vm.expectRevert("minPrice mismatch");
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), inputs, _singleMatch());
    }

    function test_submitBatch_positionLimitMismatch_reverts() public {
        pool.setPolicy(0, 0, 1e18);

        uint256[6] memory inputs;
        inputs[0] = 1;
        inputs[5] = 0;

        vm.prank(operator);
        vm.expectRevert("positionLimit mismatch");
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), inputs, _singleMatch());
    }

    // --- setFeeRecipient auth ---

    function test_setFeeRecipient_notOwner_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.setFeeRecipient(address(0x55));
    }

    // --- Withdraw event ---

    function test_withdraw_emits_event() public {
        uint256 amount = 50e18;
        _deposit(trader1, address(quoteToken), amount);

        vm.prank(trader1);
        vm.expectEmit(true, true, false, true);
        emit IDarkPool.Withdrawal(trader1, address(quoteToken), amount);
        pool.withdraw(address(quoteToken), amount);
    }

    // --- Batch size cap ---

    function test_submitBatch_tooMany_reverts() public {
        uint256[6] memory inputs;
        inputs[0] = 257;
        IDarkPool.Match[] memory matches = new IDarkPool.Match[](257);

        vm.prank(operator);
        vm.expectRevert("invalid batch size");
        pool.submitBatch(bytes32(uint256(1)), bytes32(0), new bytes(256), inputs, matches);
    }

    // --- Helpers ---

    function _deposit(address trader, address token, uint256 amount) internal {
        MockERC20(token).mint(trader, amount);
        vm.startPrank(trader);
        MockERC20(token).approve(address(pool), amount);
        pool.deposit(token, amount);
        vm.stopPrank();
    }

    function _zeroInputs() internal pure returns (uint256[6] memory inputs) {
        inputs[0] = 1;
    }

    function _singleMatch() internal view returns (IDarkPool.Match[] memory) {
        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(1)),
            askOrderId: bytes32(uint256(2)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: 0,
            size: 0
        });
        return matches;
    }

    function _fundedMatch() internal view returns (IDarkPool.Match[] memory) {
        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(1)),
            askOrderId: bytes32(uint256(2)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: 100e18,
            size: 10e18
        });
        return matches;
    }

    function _setupBatchBalances() internal {
        uint256 notional = 100e18 * 10e18 / 1e18;
        _deposit(trader1, address(quoteToken), notional);
        _deposit(trader2, address(baseToken), 10e18);
    }

    function _fundAndSubmitBatch(bytes32 batchId) internal {
        _setupBatchBalances();

        uint256[6] memory inputs;
        inputs[0] = 1;

        vm.prank(operator);
        pool.submitBatch(batchId, bytes32(0), new bytes(256), inputs, _fundedMatch());
    }
}
