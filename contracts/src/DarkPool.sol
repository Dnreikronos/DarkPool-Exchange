// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {IDarkPool} from "./interfaces/IDarkPool.sol";
import {IVerifier} from "./interfaces/IVerifier.sol";
import {IDeciderVerifier} from "./interfaces/IDeciderVerifier.sol";

contract DarkPool is IDarkPool, Ownable2Step, ReentrancyGuard, Pausable {
    using SafeERC20 for IERC20;

    uint256 public constant PROTOCOL_FEE_BPS = 5;
    uint256 public constant MAX_BATCH_SIZE = 256;
    uint256 private constant BPS_DENOMINATOR = 10_000;

    IVerifier public immutable verifier;

    mapping(address => bool) public operators;
    mapping(bytes32 => bool) public settled;
    mapping(address => mapping(address => uint256)) public balances;

    /// @notice Tokens approved for deposit. Only vetted, standard ERC20s
    ///         (no fee-on-transfer, no rebasing) should ever be allowlisted.
    ///         `deposit` rejects any token not present here, and even for
    ///         allowlisted tokens it credits the measured balance delta —
    ///         not the requested `amount` — so a token that transfers less
    ///         than requested can never overstate internal `balances`.
    mapping(address => bool) public allowedTokens;

    IDeciderVerifier public ivcVerifier;
    mapping(bytes32 => bool) public sessionSubmitted;
    mapping(bytes32 => bytes32) public auctionToSession;
    /// @notice Off-chain commitment to `keccak256(abi.encode(auctionId, matches))`
    ///         supplied at submitSession time. settleAuction re-derives this
    ///         and rejects any matches array that doesn't hash to the committed
    ///         value — prevents the operator from substituting matches between
    ///         submitSession and settleAuction. Does NOT bind the matches to
    ///         the IVC proof (see SECURITY TODO above settleAuction).
    mapping(bytes32 => bytes32) public sessionMatchesHash;

    address public feeRecipient;
    uint256 public minSize;
    uint256 public minPrice;
    uint256 public positionLimit;

    /// @notice SEC1-encoded ECIES public key advertised to clients.
    /// Updated via `setOperatorPubkey`; the off-chain decrypter
    /// continues to accept the previous key until in-flight orders
    /// drain. Stored as raw bytes (rather than uint256 pairs) so
    /// SEC1 compressed (33-byte) and uncompressed (65-byte) encodings
    /// can both be served verbatim to clients.
    bytes public operatorPubkey;

    /// @notice Block timestamp at which `operatorPubkey` becomes the
    /// primary. Pure metadata — the contract does not gate any state
    /// transition on this value; it exists so clients can avoid the
    /// thundering-herd encryption-to-old-key window during a rotation.
    uint64 public operatorPubkeyEffectiveAt;

    event SessionSubmitted(bytes32 indexed sessionId, uint64 nSteps, bytes32 policyHash);
    event AuctionSettled(bytes32 indexed sessionId, bytes32 indexed auctionId);

    modifier onlyOperator() {
        require(operators[msg.sender], "not operator");
        _;
    }

    constructor(address verifier_, address feeRecipient_, bytes memory operatorPubkey_)
        Ownable(msg.sender)
    {
        require(verifier_ != address(0), "zero verifier");
        // The verifier slot is immutable — a non-contract address would brick
        // submitBatch with no recovery path, so reject it at construction.
        require(verifier_.code.length > 0, "verifier has no code");
        require(feeRecipient_ != address(0), "zero fee recipient");
        require(
            operatorPubkey_.length == 33 || operatorPubkey_.length == 65,
            "bad pubkey len"
        );
        require(_validSec1Tag(operatorPubkey_), "bad pubkey tag");
        verifier = IVerifier(verifier_);
        feeRecipient = feeRecipient_;
        operatorPubkey = operatorPubkey_;
        operatorPubkeyEffectiveAt = uint64(block.timestamp);
        emit OperatorPubkeyUpdated(bytes(""), operatorPubkey_, uint64(block.timestamp));
    }

    /// @notice Deposit `amount` of an allowlisted ERC20 into escrow.
    /// @dev Credits the *measured* balance delta rather than the requested
    ///      `amount`. For a fee-on-transfer or rebasing token the contract
    ///      would receive less than `amount`; crediting the request would
    ///      overstate `balances` and let later withdrawals/settlements drain
    ///      other users' funds (insolvency). Measuring the delta around the
    ///      transfer makes internal accounting track real holdings exactly.
    ///      The allowlist is the primary defense (only vetted standard ERC20s
    ///      are accepted); the delta check is defense-in-depth. `nonReentrant`
    ///      guards the pre/post balanceOf reads against a malicious token that
    ///      re-enters mid-transfer.
    function deposit(address token, uint256 amount) external nonReentrant whenNotPaused {
        require(amount > 0, "zero amount");
        require(allowedTokens[token], "token not allowed");
        uint256 balanceBefore = IERC20(token).balanceOf(address(this));
        IERC20(token).safeTransferFrom(msg.sender, address(this), amount);
        uint256 received = IERC20(token).balanceOf(address(this)) - balanceBefore;
        require(received > 0, "no tokens received");
        balances[msg.sender][token] += received;
        emit Deposit(msg.sender, token, received);
    }

    /// @notice Add or remove a token from the deposit allowlist.
    /// @dev Owner-only. Allowlist only tokens that have been vetted as
    ///      standard, non-fee-on-transfer, non-rebasing ERC20s. Removing a
    ///      token blocks new deposits but does not touch existing balances —
    ///      traders can still withdraw what they already hold.
    function setTokenAllowed(address token, bool allowed) external onlyOwner {
        require(token != address(0), "zero token");
        allowedTokens[token] = allowed;
        emit TokenAllowed(token, allowed);
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

    /// @dev Uncompressed Groth16 proof layout (256 bytes): A.x, A.y, B.x.c1,
    ///      B.x.c0, B.y.c1, B.y.c0, C.x, C.y — each as a 32-byte big-endian
    ///      uint. The G2 element B MUST be serialized in (imag, real) order
    ///      to match Groth16Verifier's precompile-canonical layout; the
    ///      verifier passes B through to bn256Pairing without re-ordering.
    ///      Off-chain encoders that produce snarkjs-style (real, imag) bytes
    ///      will silently fail verification.
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

        balances[m.bidTrader][m.quoteToken] -= notional;
        balances[m.bidTrader][m.baseToken] += m.size;

        balances[m.askTrader][m.baseToken] -= m.size;
        balances[m.askTrader][m.quoteToken] += askReceives;

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

    /// @notice Set the IVC proof verifier. MUST point at VerifierProxy, not
    ///         directly at the concrete decider, so key rotation goes through
    ///         the proxy's governance without redeploying DarkPool.
    function setIvcVerifier(address ivcVerifier_) external onlyOwner {
        require(ivcVerifier_ != address(0), "zero ivc verifier");
        require(ivcVerifier_.code.length > 0, "ivc verifier has no code");
        ivcVerifier = IDeciderVerifier(ivcVerifier_);
    }

    /// @notice Commit to an IVC-proved session.
    /// @param matchesHash keccak256(abi.encode(auctionId, matches)) — the
    ///        operator's on-chain commitment to the exact matches array that
    ///        will be passed to settleAuction. Re-checked there.
    function submitSession(
        bytes32 sessionId,
        bytes calldata proof,
        uint256[3] calldata z0,
        uint256[3] calldata zN,
        uint64 nSteps,
        bytes32 policyHash,
        bytes32 matchesHash
    ) external onlyOperator whenNotPaused {
        require(!sessionSubmitted[sessionId], "session already submitted");
        require(address(ivcVerifier) != address(0), "ivc verifier not set");
        require(ivcVerifier.verifyIvcProof(proof, z0, zN, nSteps), "invalid ivc proof");
        sessionSubmitted[sessionId] = true;
        sessionMatchesHash[sessionId] = matchesHash;
        emit SessionSubmitted(sessionId, nSteps, policyHash);
    }

    /// @dev SECURITY TODO (Phase C): `matches` are still not cryptographically
    ///      bound to the IVC proof itself — `sessionMatchesHash` is an
    ///      operator-supplied commitment, not derived from `zN`. `z_n[0]`
    ///      accumulates a Poseidon chain over Pedersen commitments (private
    ///      witness), not over plaintext match tuples — on-chain verification
    ///      cannot reconstruct the commitment root without trader secrets.
    ///      Until the step circuit exposes a Poseidon hash of plaintext match
    ///      data in a public output slot and this function verifies it
    ///      against `z_n`, the matchesHash check prevents post-session
    ///      substitution but does not prove the matches were actually proved.
    function settleAuction(
        bytes32 sessionId,
        bytes32 auctionId,
        IDarkPool.Match[] calldata matches
    ) external onlyOperator whenNotPaused {
        require(sessionSubmitted[sessionId], "session not submitted");
        require(!settled[auctionId], "auction already settled");
        // Defense in depth: once an auction is bound to a session, only that
        // session can settle it. `settled[auctionId]` already blocks
        // re-settlement; this guards a cross-session race where two distinct
        // submitSession calls both try to settle the same auctionId.
        bytes32 bound = auctionToSession[auctionId];
        require(bound == bytes32(0) || bound == sessionId, "auction bound to other session");
        require(matches.length > 0 && matches.length <= MAX_BATCH_SIZE, "invalid batch size");
        // Verify the matches array matches the commitment from submitSession.
        // Re-derives keccak256(abi.encode(auctionId, matches)) and rejects any
        // substitution attempt.
        require(
            keccak256(abi.encode(auctionId, matches)) == sessionMatchesHash[sessionId],
            "matches hash mismatch"
        );
        auctionToSession[auctionId] = sessionId;
        settled[auctionId] = true;
        for (uint256 i = 0; i < matches.length; i++) {
            _settleMatch(matches[i]);
        }
        emit AuctionSettled(sessionId, auctionId);
    }

    /// @notice Publish a new operator ECIES pubkey.
    /// @dev The off-chain decrypter continues to accept the old key for
    ///      `MultiKeyDecrypter`-driven drain; this on-chain pointer is
    ///      the discovery surface for clients. SEC1 length check is the
    ///      same as the client-side validator (compressed 33 or
    ///      uncompressed 65 bytes) so an operator typo surfaces here
    ///      rather than as silent message loss.
    function setOperatorPubkey(bytes calldata newPubkey, uint64 effectiveAt) external onlyOwner {
        require(newPubkey.length == 33 || newPubkey.length == 65, "bad pubkey len");
        require(_validSec1Tag(newPubkey), "bad pubkey tag");
        bytes memory old = operatorPubkey;
        operatorPubkey = newPubkey;
        operatorPubkeyEffectiveAt = effectiveAt;
        emit OperatorPubkeyUpdated(old, newPubkey, effectiveAt);
    }

    /// @dev SEC1 leading-byte check. Compressed (33 bytes) uses 0x02
    /// or 0x03 to encode the Y parity; uncompressed (65 bytes) uses
    /// 0x04. Length is the caller's responsibility — callers must
    /// gate on `len == 33 || len == 65` before invoking this.
    function _validSec1Tag(bytes memory pubkey) internal pure returns (bool) {
        bytes1 tag = pubkey[0];
        if (pubkey.length == 33) {
            return tag == 0x02 || tag == 0x03;
        }
        return tag == 0x04;
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }
}
