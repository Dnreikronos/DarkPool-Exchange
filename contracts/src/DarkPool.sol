// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {IDarkPool} from "./interfaces/IDarkPool.sol";
import {Groth16Verifier} from "./Groth16Verifier.sol";

contract DarkPool is IDarkPool, ReentrancyGuard, Pausable {
    using SafeERC20 for IERC20;

    uint256 public constant PROTOCOL_FEE_BPS = 5;
    uint256 public constant MAX_BATCH_SIZE = 256;
    uint256 private constant BPS_DENOMINATOR = 10_000;

    address public owner;
    Groth16Verifier public verifier;

    mapping(address => bool) public operators;
    mapping(bytes32 => bool) public settled;
    mapping(address => mapping(address => uint256)) public balances;

    address public feeRecipient;
    uint256 public minSize;
    uint256 public minPrice;
    uint256 public positionLimit;

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    modifier onlyOperator() {
        require(operators[msg.sender], "not operator");
        _;
    }

    constructor(address verifier_, address feeRecipient_) {
        require(verifier_ != address(0), "zero verifier");
        require(feeRecipient_ != address(0), "zero fee recipient");
        owner = msg.sender;
        verifier = Groth16Verifier(verifier_);
        feeRecipient = feeRecipient_;
    }

    function deposit(address token, uint256 amount) external whenNotPaused {
        require(amount > 0, "zero amount");
        IERC20(token).safeTransferFrom(msg.sender, address(this), amount);
        balances[msg.sender][token] += amount;
        emit Deposit(msg.sender, token, amount);
    }

    function withdraw(address token, uint256 amount) external nonReentrant whenNotPaused {
        require(amount > 0, "zero amount");
        require(balances[msg.sender][token] >= amount, "insufficient balance");
        balances[msg.sender][token] -= amount;
        IERC20(token).safeTransfer(msg.sender, amount);
        emit Withdrawal(msg.sender, token, amount);
    }

    function submitBatch(
        bytes32 batchId,
        bytes32 auctionId,
        bytes calldata proof,
        uint256[6] calldata publicInputs,
        Match[] calldata matches
    ) external onlyOperator whenNotPaused {
        require(!settled[batchId], "already settled");
        require(matches.length > 0 && matches.length <= MAX_BATCH_SIZE, "invalid batch size");
        require(proof.length == 256, "invalid proof length");

        require(publicInputs[0] == matches.length, "match count mismatch");
        require(publicInputs[3] == minSize, "minSize mismatch");
        require(publicInputs[4] == minPrice, "minPrice mismatch");
        require(publicInputs[5] == positionLimit, "positionLimit mismatch");

        (uint256[2] memory a, uint256[2][2] memory b, uint256[2] memory c) = _decodeProof(proof);
        require(verifier.verifyProof(a, b, c, publicInputs), "invalid proof");

        for (uint256 i = 0; i < matches.length; i++) {
            _settleMatch(matches[i]);
        }

        settled[batchId] = true;
        emit BatchSettled(batchId, block.timestamp);
    }

    function _decodeProof(bytes calldata proof)
        internal
        pure
        returns (uint256[2] memory a, uint256[2][2] memory b, uint256[2] memory c)
    {
        a[0] = uint256(bytes32(proof[0:32]));
        a[1] = uint256(bytes32(proof[32:64]));
        b[0][0] = uint256(bytes32(proof[64:96]));
        b[0][1] = uint256(bytes32(proof[96:128]));
        b[1][0] = uint256(bytes32(proof[128:160]));
        b[1][1] = uint256(bytes32(proof[160:192]));
        c[0] = uint256(bytes32(proof[192:224]));
        c[1] = uint256(bytes32(proof[224:256]));
    }

    function _settleMatch(Match calldata m) internal {
        uint256 notional = m.price * m.size / 1e18;
        uint256 fee = notional * PROTOCOL_FEE_BPS / BPS_DENOMINATOR;
        uint256 askReceives = notional - fee;

        // Bid: pays quote, receives base
        balances[m.bidTrader][m.quoteToken] -= notional;
        balances[m.bidTrader][m.baseToken] += m.size;

        // Ask: pays base, receives quote minus fee
        balances[m.askTrader][m.baseToken] -= m.size;
        balances[m.askTrader][m.quoteToken] += askReceives;

        // Protocol fee from ask-side notional
        balances[feeRecipient][m.quoteToken] += fee;
    }

    function addOperator(address op) external onlyOwner {
        require(op != address(0), "zero address");
        operators[op] = true;
        emit OperatorAdded(op);
    }

    function removeOperator(address op) external onlyOwner {
        operators[op] = false;
        emit OperatorRemoved(op);
    }

    function setPolicy(uint256 minSize_, uint256 minPrice_, uint256 positionLimit_) external onlyOwner {
        minSize = minSize_;
        minPrice = minPrice_;
        positionLimit = positionLimit_;
    }

    function setFeeRecipient(address recipient) external onlyOwner {
        require(recipient != address(0), "zero address");
        feeRecipient = recipient;
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }
}
