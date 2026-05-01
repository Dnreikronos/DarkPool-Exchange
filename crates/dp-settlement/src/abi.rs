use alloy_sol_types::sol;

pub const MAX_MATCHES_PER_BATCH: usize = 256;

sol! {
    struct Match {
        bytes32 bidOrderId;
        bytes32 askOrderId;
        uint256 price;
        uint256 size;
    }

    function submitBatch(
        bytes32 batchId,
        bytes32 auctionId,
        bytes proof,
        Match[] matches
    );

    event BatchSettled(bytes32 indexed batchId);
}
