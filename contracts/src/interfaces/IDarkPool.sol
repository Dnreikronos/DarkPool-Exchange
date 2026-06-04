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
    /// @notice Operator ECIES pubkey rotation. `effectiveAt` is the
    ///         block timestamp at which the operator commits to using
    ///         the new key as the primary; the old key remains
    ///         accepted by the off-chain decrypter until in-flight
    ///         orders drain.
    event OperatorPubkeyUpdated(bytes oldPubkey, bytes newPubkey, uint64 effectiveAt);
    /// @notice Emitted when the owner adds or removes a token from the
    ///         deposit allowlist. `allowed` is the new state.
    event TokenAllowed(address indexed token, bool allowed);
    /// @notice Free balance locked into the settlement escrow (`reserved`).
    event Reserved(address indexed trader, address indexed token, uint256 amount);
    /// @notice An unlock of reserved funds was requested. `readyAt` is the
    ///         timestamp after which `releaseUnreserve` will succeed; until
    ///         then the funds stay in escrow and remain claimable by settlement.
    event UnreserveRequested(address indexed trader, address indexed token, uint256 amount, uint256 readyAt);
    /// @notice Matured reserved funds were moved back to free balance.
    event Unreserved(address indexed trader, address indexed token, uint256 amount);
    function deposit(address token, uint256 amount) external;
    function setTokenAllowed(address token, bool allowed) external;
    function withdraw(address token, uint256 amount) external;
    function reserve(address token, uint256 amount) external;
    function requestUnreserve(address token, uint256 amount) external;
    function releaseUnreserve(address token) external;
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
    function setOperatorPubkey(bytes calldata newPubkey, uint64 effectiveAt) external;
}
