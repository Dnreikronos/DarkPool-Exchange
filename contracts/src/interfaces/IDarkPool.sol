// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface IDarkPool {
    struct Match {
        bytes32 bidOrderId;
        bytes32 askOrderId;
        address bidTrader;
        address askTrader;
        address baseToken;
        address quoteToken;
        uint256 price;
        uint256 size;
    }

    event BatchSettled(bytes32 indexed batchId, uint256 timestamp);
    event Deposit(address indexed trader, address indexed token, uint256 amount);
    event Withdrawal(address indexed trader, address indexed token, uint256 amount);
    event OperatorAdded(address indexed operator);
    event OperatorRemoved(address indexed operator);

    function deposit(address token, uint256 amount) external;
    function withdraw(address token, uint256 amount) external;
    function submitBatch(
        bytes32 batchId,
        bytes32 auctionId,
        bytes calldata proof,
        uint256[6] calldata publicInputs,
        Match[] calldata matches
    ) external;
    function addOperator(address op) external;
    function removeOperator(address op) external;
    function setPolicy(uint256 minSize, uint256 minPrice, uint256 positionLimit) external;
    function setFeeRecipient(address recipient) external;
}
