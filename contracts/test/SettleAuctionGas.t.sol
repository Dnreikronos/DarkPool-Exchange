// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {DarkPool} from "../src/DarkPool.sol";
import {IDarkPool} from "../src/interfaces/IDarkPool.sol";
import {IVerifier} from "../src/interfaces/IVerifier.sol";
import {HyperNovaDeciderVerifier} from "../src/HyperNovaDeciderVerifier.sol";
import {PoseidonBN254} from "../src/PoseidonBN254.sol";
import {MockERC20} from "./mocks/MockERC20.sol";

contract GasVerifier is IVerifier {
    function verifyProof(uint256[2] calldata, uint256[2][2] calldata, uint256[2] calldata, uint256[6] calldata)
        external
        pure
        returns (bool)
    {
        return true;
    }

    function verifyProofBatch(
        uint256[2][] calldata,
        uint256[2][2][] calldata,
        uint256[2][] calldata,
        uint256[6][] calldata
    ) external pure returns (bool) {
        return true;
    }
}

/// Gas regression guard for the #209 on-chain settlement hash-chain. The bind
/// recomputes a Poseidon chain over `matches[]`, so `settleAuction` cost grows
/// ~210k gas/match. `MAX_IVC_SETTLE_MATCHES` exists to keep a full settle under
/// the block gas limit; this pins that invariant with realistic *distinct*
/// traders (cold account/storage access, unlike the warm two-trader fixtures).
///
/// Reference measurements (warm two-trader settle, this PR): 64 matches ≈19M
/// gas, 128 ≈38M, 256 ≈76M — i.e. the old shared 256 cap was unmineable here.
contract SettleAuctionGasTest is Test {
    DarkPool pool;
    MockERC20 baseToken;
    MockERC20 quoteToken;

    address operator = address(0x1);
    address feeRecipient = address(0x2);

    bytes initialPubkey =
        hex"0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798";

    uint256 constant PRICE = 100e18;
    uint256 constant SIZE = 10e18;

    function setUp() public {
        pool = new DarkPool(address(new GasVerifier()), feeRecipient, initialPubkey);
        pool.addOperator(operator);
        baseToken = new MockERC20("Base", "BASE", 18);
        quoteToken = new MockERC20("Quote", "QUOTE", 18);
        pool.setTokenAllowed(address(baseToken), true);
        pool.setTokenAllowed(address(quoteToken), true);
        pool.setIvcVerifier(address(new HyperNovaDeciderVerifier()));
        pool.setIvcEnabled(true); // #210: arm the IVC path for the gas test
    }

    function _bidTrader(uint256 i) internal pure returns (address) {
        return address(uint160(0x100000 + 2 * i));
    }

    function _askTrader(uint256 i) internal pure returns (address) {
        return address(uint160(0x100001 + 2 * i));
    }

    /// A full settle at the cap, with a distinct bid/ask trader per match, must
    /// stay well under a 30M-gas block. The bound is generous (catches a gross
    /// regression — e.g. losing the load-constants-once amortization or raising
    /// the cap — not normal compiler drift).
    function test_gas_settle_atCap_fitsBlock() public {
        uint256 n = pool.MAX_IVC_SETTLE_MATCHES();
        uint256 notional = PRICE * SIZE / 1e18;

        IDarkPool.Match[] memory matches = new IDarkPool.Match[](n);
        (uint256[3][65] memory ark, uint256[3][3] memory mds) = PoseidonBN254.loadConstants();
        uint256 acc = pool.settlementDomain();
        for (uint256 i = 0; i < n; i++) {
            address bid = _bidTrader(i);
            address ask = _askTrader(i);
            _deposit(bid, address(quoteToken), notional);
            _reserve(bid, address(quoteToken), notional);
            _deposit(ask, address(baseToken), SIZE);
            _reserve(ask, address(baseToken), SIZE);
            matches[i] = IDarkPool.Match({
                bidOrderId: bytes32(i),
                askOrderId: bytes32(i + 1),
                bidTrader: bid,
                askTrader: ask,
                baseToken: address(baseToken),
                quoteToken: address(quoteToken),
                price: PRICE,
                size: SIZE
            });
            acc = PoseidonBN254.poseidon5(
                ark, mds, acc, uint256(uint160(bid)), uint256(uint160(ask)), PRICE / 1e10, SIZE / 1e10
            );
        }

        bytes32 sessionId = bytes32(uint256(0x5e55));
        uint256[5] memory z0 = [uint256(0), uint256(0), uint256(123), pool.settlementDomain(), uint256(0)];
        uint256[5] memory zN = [uint256(999), uint256(60), uint256(123), acc, uint256(0)];
        vm.prank(operator);
        pool.submitSession(sessionId, new bytes(64), z0, zN, 60, bytes32(uint256(123)));

        vm.prank(operator);
        uint256 g0 = gasleft();
        pool.settleAuction(sessionId, bytes32(uint256(0xa0c)), matches);
        uint256 gasUsed = g0 - gasleft();
        emit log_named_uint("settleAuction gas at cap (distinct traders)", gasUsed);
        assertLt(gasUsed, 15_000_000, "settle at cap must fit comfortably in a block");
    }

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
}
