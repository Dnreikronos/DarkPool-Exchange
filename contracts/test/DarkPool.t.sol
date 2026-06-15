// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {IDarkPool} from "../src/interfaces/IDarkPool.sol";
import {IVerifier} from "../src/interfaces/IVerifier.sol";
import {HyperNovaDeciderVerifier} from "../src/HyperNovaDeciderVerifier.sol";
import {PoseidonBN254} from "../src/PoseidonBN254.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {MockERC20} from "./mocks/MockERC20.sol";
import {FeeOnTransferERC20} from "./mocks/FeeOnTransferERC20.sol";
import {NoopTransferERC20} from "./mocks/NoopTransferERC20.sol";

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

    // 33-byte SEC1-compressed placeholder; the leading byte is the
    // compression tag (02/03) followed by 32 bytes of X coordinate.
    bytes initialPubkey =
        hex"0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798";

    function setUp() public {
        verifier = new MockVerifier(true);
        pool = new DarkPool(address(verifier), feeRecipient, initialPubkey);
        pool.addOperator(operator);

        baseToken = new MockERC20("Base", "BASE", 18);
        quoteToken = new MockERC20("Quote", "QUOTE", 18);

        // Allowlist the single MVP pair so deposits are accepted. Tokens
        // not on the allowlist are rejected by deposit().
        pool.setTokenAllowed(address(baseToken), true);
        pool.setTokenAllowed(address(quoteToken), true);

        // Arm the IVC settlement path (#210) so the HyperNova session tests can
        // exercise submitSession/settleAuction. It defaults off; the dedicated
        // gate tests below disarm it (or use a fresh pool) to cover that.
        pool.setIvcEnabled(true);
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

    // --- Deposit allowlist (#164) ---

    function test_deposit_notAllowedToken_reverts() public {
        MockERC20 rogue = new MockERC20("Rogue", "RGE", 18);
        uint256 amount = 100e18;
        rogue.mint(trader1, amount);

        vm.startPrank(trader1);
        rogue.approve(address(pool), amount);
        vm.expectRevert("token not allowed");
        pool.deposit(address(rogue), amount);
        vm.stopPrank();
    }

    function test_setTokenAllowed_addsAndEmits() public {
        MockERC20 token = new MockERC20("New", "NEW", 18);
        assertFalse(pool.allowedTokens(address(token)));

        vm.expectEmit(true, false, false, true);
        emit IDarkPool.TokenAllowed(address(token), true);
        pool.setTokenAllowed(address(token), true);

        assertTrue(pool.allowedTokens(address(token)));
    }

    function test_setTokenAllowed_removesAndEmits() public {
        assertTrue(pool.allowedTokens(address(quoteToken)));

        vm.expectEmit(true, false, false, true);
        emit IDarkPool.TokenAllowed(address(quoteToken), false);
        pool.setTokenAllowed(address(quoteToken), false);

        assertFalse(pool.allowedTokens(address(quoteToken)));
    }

    function test_setTokenAllowed_notOwner_reverts() public {
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.setTokenAllowed(address(quoteToken), false);
    }

    function test_setTokenAllowed_zeroToken_reverts() public {
        vm.expectRevert("zero token");
        pool.setTokenAllowed(address(0), true);
    }

    function test_deposit_afterDelisting_reverts() public {
        pool.setTokenAllowed(address(quoteToken), false);

        uint256 amount = 100e18;
        quoteToken.mint(trader1, amount);
        vm.startPrank(trader1);
        quoteToken.approve(address(pool), amount);
        vm.expectRevert("token not allowed");
        pool.deposit(address(quoteToken), amount);
        vm.stopPrank();
    }

    function test_withdraw_afterDelisting_succeeds() public {
        // Delisting a token must not strand funds already deposited.
        uint256 amount = 100e18;
        _deposit(trader1, address(quoteToken), amount);

        pool.setTokenAllowed(address(quoteToken), false);

        vm.prank(trader1);
        pool.withdraw(address(quoteToken), amount);

        assertEq(pool.balances(trader1, address(quoteToken)), 0);
        assertEq(quoteToken.balanceOf(trader1), amount);
    }

    // --- Deposit balance-delta accounting (fee-on-transfer) (#164) ---

    function test_deposit_feeOnTransfer_creditsActualReceived() public {
        // 100 bps (1%) fee-on-transfer token. A deposit of 100e18 transfers
        // only 99e18 to the pool; balances must reflect the received 99e18,
        // not the requested 100e18, or internal accounting goes insolvent.
        FeeOnTransferERC20 fee = new FeeOnTransferERC20("Fee", "FEE", 100);
        pool.setTokenAllowed(address(fee), true);

        uint256 amount = 100e18;
        uint256 expectedReceived = amount - (amount * 100 / 10_000); // 99e18
        fee.mint(trader1, amount);

        vm.startPrank(trader1);
        fee.approve(address(pool), amount);
        pool.deposit(address(fee), amount);
        vm.stopPrank();

        assertEq(pool.balances(trader1, address(fee)), expectedReceived);
        assertEq(fee.balanceOf(address(pool)), expectedReceived);
        // Internal accounting never exceeds real holdings.
        assertEq(pool.balances(trader1, address(fee)), fee.balanceOf(address(pool)));
    }

    function test_deposit_feeOnTransfer_emitsReceivedAmount() public {
        FeeOnTransferERC20 fee = new FeeOnTransferERC20("Fee", "FEE", 100);
        pool.setTokenAllowed(address(fee), true);

        uint256 amount = 100e18;
        uint256 expectedReceived = 99e18;
        fee.mint(trader1, amount);

        vm.startPrank(trader1);
        fee.approve(address(pool), amount);
        vm.expectEmit(true, true, false, true);
        emit IDarkPool.Deposit(trader1, address(fee), expectedReceived);
        pool.deposit(address(fee), amount);
        vm.stopPrank();
    }

    function test_deposit_noTokensReceived_reverts() public {
        // A token whose transferFrom reports success but moves nothing yields
        // a zero balance delta. deposit() must revert rather than credit a
        // phantom balance.
        NoopTransferERC20 noop = new NoopTransferERC20();
        pool.setTokenAllowed(address(noop), true);

        uint256 amount = 100e18;
        noop.mint(trader1, amount);
        vm.startPrank(trader1);
        noop.approve(address(pool), amount);
        vm.expectRevert("no tokens received");
        pool.deposit(address(noop), amount);
        vm.stopPrank();

        assertEq(pool.balances(trader1, address(noop)), 0);
    }

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
        vm.expectRevert("batch already settled");
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
        DarkPool badPool = new DarkPool(address(badVerifier), feeRecipient, initialPubkey);
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

        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

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

        assertTrue(pool.settledBatch(batchId));
        assertEq(pool.balances(trader1, address(quoteToken)), 0);
        assertEq(pool.balances(trader1, address(baseToken)), size);
        assertEq(pool.balances(trader2, address(baseToken)), 0);
        assertEq(pool.balances(trader2, address(quoteToken)), notional - fee);
        assertEq(pool.balances(feeRecipient, address(quoteToken)), fee);
    }

    /// #211 regression: a 6-decimal quote token (USDC-like) must settle its
    /// quote leg in 1e6 units, not 1e18. The operator submits canonical
    /// `price`/`size` (decimal * 1e18, the #209 wire domain); `_settleMatch`
    /// rescales the notional to the quote token's own decimals. Pre-fix the
    /// contract computed `notional` 1e18-scaled (1000e18), underflowing the
    /// 1000e6 the bid actually reserved and reverting the whole batch.
    function test_submitBatch_settles6DecimalQuote() public {
        MockERC20 usdc = new MockERC20("USDC", "USDC", 6);
        pool.setTokenAllowed(address(usdc), true);

        uint256 price = 100e18; // 100.0
        uint256 size = 10e18; // 10.0
        uint256 baseAmount = 10e18; // base raw (18 decimals)
        uint256 notional = 1000e6; // 100 * 10, in 6-decimal USDC units
        uint256 fee = notional * 5 / 10_000; // 5 bps = 5e5

        _depositAndReserve(trader1, address(usdc), notional);
        _depositAndReserve(trader2, address(baseToken), baseAmount);

        uint256[6] memory inputs;
        inputs[0] = 1;

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(1)),
            askOrderId: bytes32(uint256(2)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(usdc),
            price: price,
            size: size
        });

        vm.prank(operator);
        pool.submitBatch(bytes32(uint256(0x6d)), bytes32(0), new bytes(256), inputs, matches);

        assertTrue(pool.settledBatch(bytes32(uint256(0x6d))));
        assertEq(pool.balances(trader1, address(usdc)), 0);
        assertEq(pool.balances(trader1, address(baseToken)), baseAmount);
        assertEq(pool.balances(trader2, address(baseToken)), 0);
        assertEq(pool.balances(trader2, address(usdc)), notional - fee);
        assertEq(pool.balances(feeRecipient, address(usdc)), fee);
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

        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

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
        new DarkPool(address(0), feeRecipient, initialPubkey);
    }

    function test_constructor_eoaVerifier_reverts() public {
        // The verifier slot is immutable, so a non-contract address (EOA or
        // never-deployed) would permanently brick submitBatch.
        vm.expectRevert("verifier has no code");
        new DarkPool(address(0xBEEF), feeRecipient, initialPubkey);
    }

    function test_constructor_zeroFeeRecipient_reverts() public {
        vm.expectRevert("zero fee recipient");
        new DarkPool(address(verifier), address(0), initialPubkey);
    }

    function test_constructor_emptyPubkey_reverts() public {
        vm.expectRevert("bad pubkey len");
        new DarkPool(address(verifier), feeRecipient, hex"");
    }

    function test_constructor_pubkeyTooShort_reverts() public {
        // 32 bytes — one byte short of compressed SEC1.
        bytes memory bad = new bytes(32);
        vm.expectRevert("bad pubkey len");
        new DarkPool(address(verifier), feeRecipient, bad);
    }

    function test_constructor_emitsInitialPubkeyEvent() public {
        vm.expectEmit(false, false, false, true);
        emit IDarkPool.OperatorPubkeyUpdated(bytes(""), initialPubkey, uint64(block.timestamp));
        new DarkPool(address(verifier), feeRecipient, initialPubkey);
    }

    // --- setOperatorPubkey ---

    function test_setOperatorPubkey_storesAndEmits() public {
        // Uncompressed SEC1 — 65 bytes (0x04 prefix + 32-byte X + 32-byte Y).
        bytes memory next = new bytes(65);
        next[0] = 0x04;
        for (uint256 i = 1; i < 65; i++) {
            next[i] = bytes1(uint8(i));
        }
        uint64 effectiveAt = uint64(block.timestamp + 600);

        vm.expectEmit(false, false, false, true);
        emit IDarkPool.OperatorPubkeyUpdated(initialPubkey, next, effectiveAt);
        pool.setOperatorPubkey(next, effectiveAt);

        assertEq(pool.operatorPubkey(), next);
        assertEq(pool.operatorPubkeyEffectiveAt(), effectiveAt);
    }

    function test_setOperatorPubkey_notOwner_reverts() public {
        bytes memory next = new bytes(33);
        next[0] = 0x02;
        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        pool.setOperatorPubkey(next, uint64(block.timestamp));
    }

    function test_setOperatorPubkey_badLen_reverts() public {
        // Neither 33 nor 65 bytes — must be rejected so a typo doesn't
        // brick discovery silently.
        bytes memory bad = new bytes(48);
        vm.expectRevert("bad pubkey len");
        pool.setOperatorPubkey(bad, uint64(block.timestamp));
    }

    function test_setOperatorPubkey_badCompressedTag_reverts() public {
        // 33 bytes with a tag byte that isn't 0x02 or 0x03 is not a
        // valid SEC1 compressed point — reject so an operator typo
        // doesn't silently brick client discovery.
        bytes memory bad = new bytes(33);
        for (uint256 i = 0; i < 33; i++) {
            bad[i] = 0xff;
        }
        vm.expectRevert("bad pubkey tag");
        pool.setOperatorPubkey(bad, uint64(block.timestamp));
    }

    function test_setOperatorPubkey_badUncompressedTag_reverts() public {
        // 65 bytes must start with 0x04; anything else is malformed.
        bytes memory bad = new bytes(65);
        bad[0] = 0x05;
        vm.expectRevert("bad pubkey tag");
        pool.setOperatorPubkey(bad, uint64(block.timestamp));
    }

    function test_setOperatorPubkey_acceptsBothCompressedTags() public {
        // 0x02 and 0x03 (Y-parity) must both be accepted.
        bytes memory withTwo = new bytes(33);
        withTwo[0] = 0x02;
        for (uint256 i = 1; i < 33; i++) {
            withTwo[i] = bytes1(uint8(i));
        }
        pool.setOperatorPubkey(withTwo, uint64(block.timestamp));
        assertEq(pool.operatorPubkey(), withTwo);

        bytes memory withThree = new bytes(33);
        withThree[0] = 0x03;
        for (uint256 i = 1; i < 33; i++) {
            withThree[i] = bytes1(uint8(i));
        }
        pool.setOperatorPubkey(withThree, uint64(block.timestamp));
        assertEq(pool.operatorPubkey(), withThree);
    }

    function test_constructor_badPubkeyTag_reverts() public {
        bytes memory bad = new bytes(33);
        for (uint256 i = 0; i < 33; i++) {
            bad[i] = 0xff;
        }
        vm.expectRevert("bad pubkey tag");
        new DarkPool(address(verifier), feeRecipient, bad);
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

    // --- HyperNova IVC ---

    function test_submit_hypernova_session() public {
        // Deploy and set the IVC verifier (stub returns true always)
        HyperNovaDeciderVerifier ivcVerifier = new HyperNovaDeciderVerifier();
        pool.setIvcVerifier(address(ivcVerifier));

        bytes32 sessionId = bytes32(uint256(1));
        bytes memory proof = new bytes(64); // dummy proof
        uint64 nSteps = 60;
        bytes32 policyHash = bytes32(TEST_POLICY_HASH);

        // Fund + reserve traders so _settleMatch debits locked escrow. Amounts
        // are in the on-chain wei domain (decimal * 1e18) so they scale down
        // cleanly to the circuit's 1e8 domain.
        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

        bytes32 auctionId = bytes32(uint256(2));
        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(10)),
            askOrderId: bytes32(uint256(11)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: price,
            size: size
        });

        // The proof binds the settled matches via zN[3] = the settlement
        // hash-chain over (bidTrader, askTrader, price_1e8, size_1e8). An honest
        // operator's zN[3] equals what settleAuction recomputes from matches[].
        uint256 settlementAcc = _settlementAcc(trader1, trader2, price, size);
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(settlementAcc);

        vm.prank(operator);
        pool.submitSession(sessionId, proof, z0, zN, nSteps, policyHash);
        assertTrue(pool.sessionSubmitted(sessionId));
        assertEq(pool.sessionSettlementAcc(sessionId), settlementAcc);

        // settleAuction with the proved match
        vm.prank(operator);
        pool.settleAuction(sessionId, auctionId, matches);
        assertTrue(pool.settledAuction(auctionId));
    }

    /// #210: the IVC settlement path is disarmed on a freshly constructed pool,
    /// so wiring the all-accepting stub decider cannot auto-settle escrow.
    function test_ivc_disabledByDefault() public {
        DarkPool fresh = new DarkPool(address(verifier), feeRecipient, initialPubkey);
        assertFalse(fresh.ivcEnabled());
    }

    /// Only the owner can arm/disarm the IVC path, and doing so emits the event.
    function test_setIvcEnabled_onlyOwner_and_emits() public {
        DarkPool fresh = new DarkPool(address(verifier), feeRecipient, initialPubkey);

        vm.prank(address(0xdead));
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(0xdead)));
        fresh.setIvcEnabled(true);

        vm.expectEmit(false, false, false, true);
        emit DarkPool.IvcEnabledSet(true);
        fresh.setIvcEnabled(true);
        assertTrue(fresh.ivcEnabled());

        fresh.setIvcEnabled(false);
        assertFalse(fresh.ivcEnabled());
    }

    /// submitSession reverts while the IVC path is disarmed, even with a verifier
    /// wired and a valid operator — the gate is independent of both.
    function test_submitSession_revertsWhenIvcDisabled() public {
        pool.setIvcVerifier(address(new HyperNovaDeciderVerifier()));
        pool.setIvcEnabled(false);

        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(0);
        vm.prank(operator);
        vm.expectRevert("ivc settlement disabled");
        pool.submitSession(bytes32(uint256(1)), new bytes(64), z0, zN, 60, bytes32(TEST_POLICY_HASH));
    }

    /// Disarming after a session is submitted also freezes settlement: the whole
    /// IVC path, not just its entry point, is gated.
    function test_settleAuction_revertsWhenIvcDisabled() public {
        pool.setIvcVerifier(address(new HyperNovaDeciderVerifier()));

        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

        bytes32 sessionId = bytes32(uint256(1));
        uint256 settlementAcc = _settlementAcc(trader1, trader2, price, size);
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(settlementAcc);
        vm.prank(operator);
        pool.submitSession(sessionId, new bytes(64), z0, zN, 60, bytes32(TEST_POLICY_HASH));

        // Owner disarms the path after submission; settlement must now revert.
        pool.setIvcEnabled(false);

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(10)),
            askOrderId: bytes32(uint256(11)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: price,
            size: size
        });
        vm.prank(operator);
        vm.expectRevert("ivc settlement disabled");
        pool.settleAuction(sessionId, bytes32(uint256(2)), matches);
    }

    /// The #209 binding: the proof fixes zN[3] to the settlement chain over the
    /// matches it proved. Settling any other matches array makes the on-chain
    /// recompute diverge from zN[3], so settlement reverts.
    function test_settleAuction_substitutedMatches_reverts() public {
        HyperNovaDeciderVerifier ivcVerifier = new HyperNovaDeciderVerifier();
        pool.setIvcVerifier(address(ivcVerifier));

        bytes32 sessionId = bytes32(uint256(1));
        bytes memory proof = new bytes(64);
        uint64 nSteps = 60;
        bytes32 policyHash = bytes32(TEST_POLICY_HASH);
        bytes32 auctionId = bytes32(uint256(2));

        uint256 price = 100e18;
        uint256 size = 10e18;
        IDarkPool.Match[] memory provedMatches = new IDarkPool.Match[](1);
        provedMatches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(10)),
            askOrderId: bytes32(uint256(11)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: price,
            size: size
        });

        // zN[3] binds exactly the proved matches.
        uint256 settlementAcc = _settlementAcc(trader1, trader2, price, size);
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(settlementAcc);

        vm.prank(operator);
        pool.submitSession(sessionId, proof, z0, zN, nSteps, policyHash);

        // Settle a repriced match (still 1e10-divisible). Recomputed chain != zN[3].
        IDarkPool.Match[] memory substituted = new IDarkPool.Match[](1);
        substituted[0] = provedMatches[0];
        substituted[0].size = 20e18; // tamper

        vm.prank(operator);
        vm.expectRevert("settlement binding");
        pool.settleAuction(sessionId, auctionId, substituted);
    }

    /// A match carrying sub-1e8 precision (price/size not divisible by the
    /// wei→circuit scale) could not have been proved, so the binding rejects it
    /// before folding it into the chain.
    function test_settleAuction_subCircuitPrecision_reverts() public {
        HyperNovaDeciderVerifier ivcVerifier = new HyperNovaDeciderVerifier();
        pool.setIvcVerifier(address(ivcVerifier));

        bytes32 sessionId = bytes32(uint256(1));
        bytes memory proof = new bytes(64);
        uint64 nSteps = 60;
        bytes32 policyHash = bytes32(TEST_POLICY_HASH);
        bytes32 auctionId = bytes32(uint256(2));

        // zN[3] is a dummy here; the precision check reverts before the binding.
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(1);
        vm.prank(operator);
        pool.submitSession(sessionId, proof, z0, zN, nSteps, policyHash);

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(10)),
            askOrderId: bytes32(uint256(11)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: 100e18 + 1, // not divisible by WEI_TO_CIRCUIT_SCALE (1e10)
            size: 10e18
        });

        vm.prank(operator);
        vm.expectRevert("match precision");
        pool.settleAuction(sessionId, auctionId, matches);
    }

    /// #209 replay guard: a session proves one auction's matches, and the
    /// settlement binding is over matches[] only — auctionId is absent from the
    /// proof's hash-chain. Without a per-session settled flag the operator could
    /// settle the SAME proved matches under a fresh auctionId, re-running
    /// _settleMatch and double-spending reserved escrow. The second settle must
    /// revert even when the escrow could fund it.
    function test_settleAuction_sessionReplay_reverts() public {
        HyperNovaDeciderVerifier ivcVerifier = new HyperNovaDeciderVerifier();
        pool.setIvcVerifier(address(ivcVerifier));

        bytes32 sessionId = bytes32(uint256(1));
        bytes memory proof = new bytes(64);
        uint64 nSteps = 60;
        bytes32 policyHash = bytes32(TEST_POLICY_HASH);

        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;
        // Reserve enough escrow for TWO settlements so the replay would succeed
        // financially if the guard were missing — isolating the per-session
        // flag as the thing that stops it, not an escrow underflow.
        _depositAndReserve(trader1, address(quoteToken), notional * 2);
        _depositAndReserve(trader2, address(baseToken), size * 2);

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](1);
        matches[0] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(10)),
            askOrderId: bytes32(uint256(11)),
            bidTrader: trader1,
            askTrader: trader2,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: price,
            size: size
        });

        uint256 settlementAcc = _settlementAcc(trader1, trader2, price, size);
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(settlementAcc);

        vm.prank(operator);
        pool.submitSession(sessionId, proof, z0, zN, nSteps, policyHash);

        // First settlement succeeds and marks the session settled.
        vm.prank(operator);
        pool.settleAuction(sessionId, bytes32(uint256(2)), matches);
        assertTrue(pool.settledAuction(bytes32(uint256(2))));
        assertTrue(pool.sessionSettled(sessionId));

        // Replaying the same proved matches under a fresh auctionId clears the
        // matches-only binding but must hit the per-session guard.
        vm.prank(operator);
        vm.expectRevert("session already settled");
        pool.settleAuction(sessionId, bytes32(uint256(3)), matches);
    }

    /// The IVC settle path caps matches well below MAX_BATCH_SIZE: the on-chain
    /// Poseidon recompute makes a 256-match settle exceed the block gas limit, so
    /// the operator must be stopped from broadcasting an unmineable settle tx. One
    /// past the cap reverts; the size guard fires before the binding or any
    /// _settleMatch, so the matches need no funding.
    function test_settleAuction_tooManyMatches_reverts() public {
        HyperNovaDeciderVerifier ivcVerifier = new HyperNovaDeciderVerifier();
        pool.setIvcVerifier(address(ivcVerifier));

        bytes32 sessionId = bytes32(uint256(1));
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(123);
        vm.prank(operator);
        pool.submitSession(sessionId, new bytes(64), z0, zN, 60, bytes32(TEST_POLICY_HASH));

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](pool.MAX_IVC_SETTLE_MATCHES() + 1);
        vm.prank(operator);
        vm.expectRevert("invalid ivc batch size");
        pool.settleAuction(sessionId, bytes32(uint256(2)), matches);
    }

    /// Defense in depth (#209): submitSession pins the proof's public IO to its
    /// canonical form. A non-zero z0[3] (the settlement accumulator must start at
    /// zero, where settleAuction's recompute begins) is rejected up front.
    function test_submitSession_nonZeroZ0SettlementAcc_reverts() public {
        HyperNovaDeciderVerifier ivcVerifier = new HyperNovaDeciderVerifier();
        pool.setIvcVerifier(address(ivcVerifier));

        uint256[5] memory z0 = [uint256(0), uint256(0), TEST_POLICY_HASH, uint256(7), uint256(0)];
        uint256[5] memory zN = [uint256(999), uint256(60), TEST_POLICY_HASH, uint256(123), uint256(0)];

        vm.prank(operator);
        vm.expectRevert("z0 settlement acc not zero");
        pool.submitSession(bytes32(uint256(1)), new bytes(64), z0, zN, 60, bytes32(TEST_POLICY_HASH));
    }

    /// The emitted policyHash must equal the proved zN[2] (policy is invariant
    /// across IVC steps), so a watcher can trust the SessionSubmitted event.
    function test_submitSession_policyHashMismatch_reverts() public {
        HyperNovaDeciderVerifier ivcVerifier = new HyperNovaDeciderVerifier();
        pool.setIvcVerifier(address(ivcVerifier));

        // zN[2] == TEST_POLICY_HASH (123); pass a different policyHash arg.
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(123);
        vm.prank(operator);
        vm.expectRevert("policy hash mismatch");
        pool.submitSession(bytes32(uint256(1)), new bytes(64), z0, zN, 60, bytes32(uint256(999)));
    }

    // --- Cross-path double-settlement (#214) ---

    /// The Groth16 and IVC arms share `settledAuction`, so a round settled via
    /// submitBatch cannot be re-settled via settleAuction. The guard fires
    /// before the binding or any _settleMatch, so the same proved matches
    /// (which would otherwise pass the #209 binding) are still rejected.
    function test_groth16ThenIvc_sameAuction_reverts() public {
        bytes32 auctionId = bytes32(uint256(0xA));
        bytes32 batchId = bytes32(uint256(0xB));

        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

        // Groth16 settles the economic round.
        vm.prank(operator);
        pool.submitBatch(batchId, auctionId, new bytes(256), _zeroInputs(), _fundedMatch());
        assertTrue(pool.settledAuction(auctionId));

        // IVC submits the SAME matches (binding would pass) and tries to settle
        // the same auctionId — the shared guard, not the binding, must stop it.
        pool.setIvcVerifier(address(new HyperNovaDeciderVerifier()));
        bytes32 sessionId = bytes32(uint256(0xC));
        uint256 settlementAcc = _settlementAcc(trader1, trader2, price, size);
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(settlementAcc);
        vm.prank(operator);
        pool.submitSession(sessionId, new bytes(64), z0, zN, 60, bytes32(TEST_POLICY_HASH));

        vm.prank(operator);
        vm.expectRevert("auction already settled");
        pool.settleAuction(sessionId, auctionId, _fundedMatch());
    }

    /// Mirror of the above: a round settled via the IVC arm cannot be
    /// re-settled via submitBatch. settleAuction marks `settledAuction`, which
    /// submitBatch checks before touching escrow.
    function test_ivcThenGroth16_sameAuction_reverts() public {
        bytes32 auctionId = bytes32(uint256(0xA));

        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

        pool.setIvcVerifier(address(new HyperNovaDeciderVerifier()));
        bytes32 sessionId = bytes32(uint256(0xC));
        uint256 settlementAcc = _settlementAcc(trader1, trader2, price, size);
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(settlementAcc);
        vm.prank(operator);
        pool.submitSession(sessionId, new bytes(64), z0, zN, 60, bytes32(TEST_POLICY_HASH));
        vm.prank(operator);
        pool.settleAuction(sessionId, auctionId, _fundedMatch());
        assertTrue(pool.settledAuction(auctionId));

        vm.prank(operator);
        vm.expectRevert("auction already settled");
        pool.submitBatch(bytes32(uint256(0xB)), auctionId, new bytes(256), _zeroInputs(), _fundedMatch());
    }

    /// `batchId` and `auctionId` live in separate namespaces, so a Groth16 batch
    /// settled under id K never blocks an unrelated IVC auction whose auctionId
    /// equals K. Under the old shared `settled` map this coincidental collision
    /// would have falsely reverted the second, legitimate settlement.
    function test_namespaceSeparation_batchIdDoesNotBlockEqualAuctionId() public {
        bytes32 sharedId = bytes32(uint256(0x7777));

        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;

        // Groth16 batch settled under batchId == sharedId, with a distinct
        // auctionId so the round guards don't interfere.
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);
        vm.prank(operator);
        pool.submitBatch(sharedId, bytes32(uint256(0x1111)), new bytes(256), _zeroInputs(), _fundedMatch());
        assertTrue(pool.settledBatch(sharedId));
        assertFalse(pool.settledAuction(sharedId));

        // A separate IVC auction whose auctionId == sharedId must still settle.
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);
        pool.setIvcVerifier(address(new HyperNovaDeciderVerifier()));
        bytes32 sessionId = bytes32(uint256(0xC));
        uint256 settlementAcc = _settlementAcc(trader1, trader2, price, size);
        (uint256[5] memory z0, uint256[5] memory zN) = _ivcStates(settlementAcc);
        vm.prank(operator);
        pool.submitSession(sessionId, new bytes(64), z0, zN, 60, bytes32(TEST_POLICY_HASH));
        vm.prank(operator);
        pool.settleAuction(sessionId, sharedId, _fundedMatch());

        assertTrue(pool.settledAuction(sharedId));
    }

    // --- Escrow reserve / unbonding (#165) ---

    function test_reserve_movesFreeToReserved() public {
        _deposit(trader1, address(quoteToken), 100e18);

        vm.prank(trader1);
        pool.reserve(address(quoteToken), 40e18);

        assertEq(pool.balances(trader1, address(quoteToken)), 60e18);
        assertEq(pool.reserved(trader1, address(quoteToken)), 40e18);
    }

    function test_reserve_emitsEvent() public {
        uint256 amount = 100e18;
        _deposit(trader1, address(quoteToken), amount);

        vm.prank(trader1);
        vm.expectEmit(true, true, false, true);
        emit IDarkPool.Reserved(trader1, address(quoteToken), amount);
        pool.reserve(address(quoteToken), amount);
    }

    function test_reserve_insufficientFree_reverts() public {
        _deposit(trader1, address(quoteToken), 10e18);

        vm.prank(trader1);
        vm.expectRevert("insufficient balance");
        pool.reserve(address(quoteToken), 11e18);
    }

    function test_reserve_zero_reverts() public {
        vm.prank(trader1);
        vm.expectRevert("zero amount");
        pool.reserve(address(quoteToken), 0);
    }

    function test_reserve_whenPaused_reverts() public {
        pool.pause();
        vm.prank(trader1);
        vm.expectRevert(abi.encodeWithSignature("EnforcedPause()"));
        pool.reserve(address(quoteToken), 1);
    }

    function test_withdraw_cannotTouchReservedFunds() public {
        uint256 amount = 100e18;
        _deposit(trader1, address(quoteToken), amount);
        _reserve(trader1, address(quoteToken), amount); // all reserved, free == 0

        vm.prank(trader1);
        vm.expectRevert("insufficient balance");
        pool.withdraw(address(quoteToken), 1);
    }

    function test_requestUnreserve_exceedingReserved_reverts() public {
        _deposit(trader1, address(quoteToken), 100e18);
        _reserve(trader1, address(quoteToken), 40e18);

        vm.prank(trader1);
        vm.expectRevert("exceeds reserved");
        pool.requestUnreserve(address(quoteToken), 41e18);
    }

    function test_releaseUnreserve_nothingPending_reverts() public {
        vm.prank(trader1);
        vm.expectRevert("nothing pending");
        pool.releaseUnreserve(address(quoteToken));
    }

    function test_releaseUnreserve_beforeDelay_reverts() public {
        _deposit(trader1, address(quoteToken), 100e18);
        _reserve(trader1, address(quoteToken), 100e18);

        vm.startPrank(trader1);
        pool.requestUnreserve(address(quoteToken), 100e18);
        vm.expectRevert("unreserve not ready");
        pool.releaseUnreserve(address(quoteToken));
        vm.stopPrank();
    }

    function test_requestThenReleaseUnreserve_afterDelay_returnsToFree() public {
        _deposit(trader1, address(quoteToken), 100e18);
        _reserve(trader1, address(quoteToken), 100e18);

        vm.prank(trader1);
        pool.requestUnreserve(address(quoteToken), 100e18);

        vm.warp(block.timestamp + pool.UNRESERVE_DELAY());

        vm.prank(trader1);
        pool.releaseUnreserve(address(quoteToken));

        assertEq(pool.reserved(trader1, address(quoteToken)), 0);
        assertEq(pool.balances(trader1, address(quoteToken)), 100e18);
        assertEq(pool.pendingUnreserve(trader1, address(quoteToken)), 0);

        // Released funds are withdrawable again.
        vm.prank(trader1);
        pool.withdraw(address(quoteToken), 100e18);
        assertEq(quoteToken.balanceOf(trader1), 100e18);
    }

    /// The core #165 regression: a matched trader who tries to pull committed
    /// funds out before settlement is blocked by the cooldown — the funds stay
    /// in `reserved` and settlement still succeeds. One trader can no longer
    /// revert the whole batch by front-running settlement with a withdrawal.
    function test_matchedFundsRemainClaimableDuringUnbond() public {
        bytes32 batchId = bytes32(uint256(7));
        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;
        uint256 fee = notional * 5 / 10_000;

        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

        // Both traders immediately try to unbond their committed funds.
        vm.prank(trader1);
        pool.requestUnreserve(address(quoteToken), notional);
        vm.prank(trader2);
        pool.requestUnreserve(address(baseToken), size);

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

        uint256[6] memory inputs;
        inputs[0] = 1;

        // Settlement succeeds despite the pending unbonds.
        vm.prank(operator);
        pool.submitBatch(batchId, bytes32(0), new bytes(256), inputs, matches);

        assertTrue(pool.settledBatch(batchId));
        assertEq(pool.reserved(trader1, address(quoteToken)), 0);
        assertEq(pool.balances(trader1, address(baseToken)), size);
        assertEq(pool.reserved(trader2, address(baseToken)), 0);
        assertEq(pool.balances(trader2, address(quoteToken)), notional - fee);

        // Settlement consumed the reserved funds and retired the pending
        // request with them: nothing is left to release, and no stale timer
        // lingers to unlock future reserves early.
        assertEq(pool.pendingUnreserve(trader1, address(quoteToken)), 0);
        assertEq(pool.unreserveReadyAt(trader1, address(quoteToken)), 0);
        vm.warp(block.timestamp + pool.UNRESERVE_DELAY());
        vm.prank(trader1);
        vm.expectRevert("nothing pending");
        pool.releaseUnreserve(address(quoteToken));
        assertEq(pool.reserved(trader1, address(quoteToken)), 0);
        assertEq(pool.balances(trader1, address(quoteToken)), 0);
    }

    function test_submitBatch_revertsWhenFundsNotReserved() public {
        // Funds sit in free balance but were never reserved → settlement
        // debits the empty reserved escrow and reverts. Locking is enforced:
        // free balance alone cannot settle.
        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;

        _deposit(trader1, address(quoteToken), notional); // NOT reserved
        _deposit(trader2, address(baseToken), size); // NOT reserved

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

        uint256[6] memory inputs;
        inputs[0] = 1;

        vm.prank(operator);
        vm.expectRevert(); // arithmetic underflow on reserved escrow
        pool.submitBatch(bytes32(uint256(8)), bytes32(0), new bytes(256), inputs, matches);
    }

    /// The issue's exact scenario: one underfunded trader in a multi-match
    /// batch. The whole batch reverts atomically; the valid first match must
    /// NOT be partially settled (we reject the "skip the bad match" fix).
    function test_submitBatch_oneUnderfundedRevertsEntireBatch() public {
        address trader3 = address(0x30);
        address trader4 = address(0x40);

        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;

        // Match 0: both sides fully reserved (would settle on its own).
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);
        // Match 1: ask side reserved, bid side (trader3) never reserved quote.
        _depositAndReserve(trader4, address(baseToken), size);

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](2);
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
        matches[1] = IDarkPool.Match({
            bidOrderId: bytes32(uint256(3)),
            askOrderId: bytes32(uint256(4)),
            bidTrader: trader3, // underfunded: never reserved quote
            askTrader: trader4,
            baseToken: address(baseToken),
            quoteToken: address(quoteToken),
            price: price,
            size: size
        });

        uint256[6] memory inputs;
        inputs[0] = 2;

        bytes32 batchId = bytes32(uint256(9));
        vm.prank(operator);
        vm.expectRevert();
        pool.submitBatch(batchId, bytes32(0), new bytes(256), inputs, matches);

        // Atomicity: the valid first match did NOT partially settle.
        assertFalse(pool.settledBatch(batchId));
        assertEq(pool.reserved(trader1, address(quoteToken)), notional);
        assertEq(pool.balances(trader1, address(baseToken)), 0);
        assertEq(pool.reserved(trader2, address(baseToken)), size);
    }

    /// #165 stale-unbond regression: settlement that consumes reserved funds
    /// must also retire the matching pendingUnreserve and its maturity timer.
    /// Otherwise a trader could arm an unbond, let a match consume it, reserve
    /// fresh funds, and yank them out the instant the stale timer matures —
    /// front-running the next settlement with no fresh cooldown.
    function test_settlementRetiresPendingUnreserve_freshReserveStaysLocked() public {
        uint256 price = 100e18;
        uint256 size = 10e18;
        uint256 notional = price * size / 1e18;

        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), size);

        // trader1 arms an unbond on the exact funds about to be matched.
        vm.prank(trader1);
        pool.requestUnreserve(address(quoteToken), notional);

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
        uint256[6] memory inputs;
        inputs[0] = 1;

        // Settlement consumes trader1's reserved quote; the pending request and
        // its timer must be cleared, not left dangling.
        vm.prank(operator);
        pool.submitBatch(bytes32(uint256(11)), bytes32(0), new bytes(256), inputs, matches);
        assertEq(pool.reserved(trader1, address(quoteToken)), 0);
        assertEq(pool.pendingUnreserve(trader1, address(quoteToken)), 0);
        assertEq(pool.unreserveReadyAt(trader1, address(quoteToken)), 0);

        // trader1 reserves fresh quote to trade again.
        _depositAndReserve(trader1, address(quoteToken), notional);

        // Past the ORIGINAL deadline the fresh reserve must NOT be releasable:
        // settlement retired the stale request, so nothing is pending.
        vm.warp(block.timestamp + pool.UNRESERVE_DELAY());
        vm.prank(trader1);
        vm.expectRevert("nothing pending");
        pool.releaseUnreserve(address(quoteToken));
        assertEq(pool.reserved(trader1, address(quoteToken)), notional);
    }

    // --- Helpers ---

    function _deposit(address trader, address token, uint256 amount) internal {
        MockERC20(token).mint(trader, amount);
        vm.startPrank(trader);
        MockERC20(token).approve(address(pool), amount);
        pool.deposit(token, amount);
        vm.stopPrank();
    }

    function _reserve(address trader, address token, uint256 amount) internal {
        vm.prank(trader);
        pool.reserve(token, amount);
    }

    /// Settlement debits the locked `reserved` escrow, so any trader appearing
    /// in a settled match must have reserved the funds they owe first.
    function _depositAndReserve(address trader, address token, uint256 amount) internal {
        _deposit(trader, token, amount);
        _reserve(trader, token, amount);
    }

    // HyperNova session fixtures. The IVC proof is a stub, so z0/zN are fixed
    // dummies except zN[3] — the settlement accumulator settleAuction recomputes
    // from matches[]. Deriving these in one place keeps the session tests from
    // drifting on the policy hash, the 1e10 wei->circuit scale, or the 5-element
    // state layout.
    uint256 internal constant TEST_POLICY_HASH = 123;

    function _settlementAcc(address bidTrader, address askTrader, uint256 price, uint256 size)
        internal
        pure
        returns (uint256)
    {
        return PoseidonBN254.poseidon5(
            0, uint256(uint160(bidTrader)), uint256(uint160(askTrader)), price / 1e10, size / 1e10
        );
    }

    function _ivcStates(uint256 settlementAcc)
        internal
        pure
        returns (uint256[5] memory z0, uint256[5] memory zN)
    {
        z0 = [uint256(0), uint256(0), TEST_POLICY_HASH, uint256(0), uint256(0)];
        zN = [uint256(999), uint256(60), TEST_POLICY_HASH, settlementAcc, uint256(0)];
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
        _depositAndReserve(trader1, address(quoteToken), notional);
        _depositAndReserve(trader2, address(baseToken), 10e18);
    }

    function _fundAndSubmitBatch(bytes32 batchId) internal {
        _setupBatchBalances();

        uint256[6] memory inputs;
        inputs[0] = 1;

        vm.prank(operator);
        pool.submitBatch(batchId, bytes32(0), new bytes(256), inputs, _fundedMatch());
    }
}
