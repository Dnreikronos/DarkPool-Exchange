// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {IDarkPool} from "./interfaces/IDarkPool.sol";
import {IVerifier} from "./interfaces/IVerifier.sol";
import {IDeciderVerifier} from "./interfaces/IDeciderVerifier.sol";
import {PoseidonBN254} from "./PoseidonBN254.sol";

contract DarkPool is IDarkPool, Ownable2Step, ReentrancyGuard, Pausable {
    using SafeERC20 for IERC20;

    uint256 public constant PROTOCOL_FEE_BPS = 5;
    uint256 public constant MAX_BATCH_SIZE = 256;
    /// @notice Per-call match cap for the IVC settlement path (`settleAuction`),
    ///         far below `MAX_BATCH_SIZE`. The #209 binding recomputes the
    ///         Poseidon settlement chain on-chain — ~210k gas/match (three
    ///         permutations), against the Groth16 path's O(1) public-input bind.
    ///         Measured `settleAuction` gas runs ≈10M at 32 matches, ≈19M at 64,
    ///         ≈38M at 128, ≈76M at 256, so the shared 256 cap is unmineable on
    ///         this path. 32 keeps a full settle near a third of a 30M block with
    ///         headroom for the cold-account access of distinct traders (the
    ///         chain itself is trader-independent and dominates the cost).
    ///         Revisit against the production block gas limit and real auction
    ///         sizes when the real decider replaces the stub (#210).
    uint256 public constant MAX_IVC_SETTLE_MATCHES = 32;
    uint256 private constant BPS_DENOMINATOR = 10_000;

    /// @notice Bridges the on-chain amount domain (wei, `decimal * 1e18` as
    ///         produced by the operator's `decimal_to_wei`) to the circuit's 1e8
    ///         fixed-point domain (`decimal_to_scalar`). `settleAuction` divides
    ///         `Match.price`/`size` by this before folding them into the #209
    ///         settlement hash-chain, so the on-chain recompute hashes exactly
    ///         the values the circuit proved. MUST move in lockstep with the
    ///         off-chain decimal handling (#211): if per-token decimal scaling
    ///         changes, this bridge changes with it.
    uint256 private constant WEI_TO_CIRCUIT_SCALE = 1e10;

    IVerifier public immutable verifier;

    mapping(address => bool) public operators;

    /// @notice Replay guard for the Groth16 path, keyed by `batchId`. Kept in a
    ///         namespace distinct from `settledAuction` so a `batchId` can never
    ///         alias an `auctionId` of the same 32-byte value (#214). A single
    ///         shared map would either let one economic round settle twice
    ///         (different keys, no overlap) or falsely block an unrelated round
    ///         (coincidental key collision).
    mapping(bytes32 => bool) public settledBatch;

    /// @notice Replay guard for the economic auction round, keyed by `auctionId`
    ///         and SHARED across both settlement arms. `submitBatch` (Groth16)
    ///         and `settleAuction` (HyperNova IVC) each set and check it, so a
    ///         round settled through one arm cannot be re-settled through the
    ///         other (#214). The off-chain operator runs one auction → one batch
    ///         → one `submitBatch`, so a one-settlement-per-`auctionId` rule does
    ///         not constrain any legitimate flow. Without this, the same proved
    ///         matches could run `_settleMatch` twice — once via `submitBatch`,
    ///         once via `submitSession` + `settleAuction` — double-charging
    ///         traders.
    mapping(bytes32 => bool) public settledAuction;

    mapping(address => mapping(address => uint256)) public balances;

    /// @notice Tokens approved for deposit. Only vetted, standard ERC20s
    ///         (no fee-on-transfer, no rebasing) should ever be allowlisted.
    ///         `deposit` rejects any token not present here, and even for
    ///         allowlisted tokens it credits the measured balance delta —
    ///         not the requested `amount` — so a token that transfers less
    ///         than requested can never overstate internal `balances`.
    mapping(address => bool) public allowedTokens;

    /// @notice Funds locked for trading. Settlement (`_settleMatch`) debits
    ///         *only* this bucket, never free `balances`. A trader must
    ///         `reserve` before their orders are eligible to be matched.
    ///         Reserved funds are not directly withdrawable; releasing them is
    ///         gated by `UNRESERVE_DELAY`. This is the on-chain escrow that
    ///         "cannot drop before settlement" (#165): a matched trader cannot
    ///         pull committed funds out within the cooldown window, so a single
    ///         underfunded trader can no longer revert the whole batch by
    ///         front-running settlement with a withdrawal.
    mapping(address => mapping(address => uint256)) public reserved;

    /// @notice Amount of `reserved` a trader has asked to unlock, and the
    ///         timestamp at which it becomes releasable. Pending funds REMAIN
    ///         in `reserved` (settlement can still claim them) until
    ///         `releaseUnreserve` moves them back to free `balances` — so a
    ///         pending request never lets a matched trader escape settlement.
    mapping(address => mapping(address => uint256)) public pendingUnreserve;
    mapping(address => mapping(address => uint256)) public unreserveReadyAt;

    /// @notice Cooldown between requesting an unreserve and releasing it. MUST
    ///         exceed the maximum matching→settlement latency so funds matched
    ///         in an auction are still present when that auction settles.
    ///         Auctions tick every ~5s and in-browser ZK proving adds seconds;
    ///         one hour is a generous safety margin.
    uint256 public constant UNRESERVE_DELAY = 1 hours;

    IDeciderVerifier public ivcVerifier;
    mapping(bytes32 => bool) public sessionSubmitted;
    mapping(bytes32 => bytes32) public auctionToSession;
    /// @notice The proof's settlement hash-chain `z_n[3]`, recorded at
    ///         submitSession time (#209). `settleAuction` recomputes the
    ///         identical Poseidon chain over the submitted `matches[]` and
    ///         rejects any array that doesn't fold to this value — binding the
    ///         settled matches to the proof rather than to an operator-supplied
    ///         commitment. Cryptographic enforcement is gated on a real decider
    ///         verifier replacing the stub (#210); until then `z_n` is
    ///         operator-controlled.
    mapping(bytes32 => uint256) public sessionSettlementAcc;
    /// @notice Set once a session has settled an auction. The #209 settlement
    ///         binding is over `matches[]` only — auctionId is not part of the
    ///         proof's hash-chain (`zN[3]`) — so this per-session flag is what
    ///         stops one proved session from settling more than one auctionId.
    ///         Without it the operator could replay the same proved matches
    ///         under a fresh auctionId, re-running `_settleMatch` and
    ///         double-spending reserved escrow.
    mapping(bytes32 => bool) public sessionSettled;

    /// @notice IVC settlement arm switch. `submitSession` and `settleAuction`
    ///         revert while this is false, so wiring the all-accepting stub
    ///         decider no longer auto-arms fund settlement on the IVC path
    ///         (#210). Defaults to false at construction; the owner flips it on
    ///         only once a real, audited Decider verifier has replaced the stub.
    ///         Defense-in-depth behind the deploy-time chain allowlist: a stub
    ///         wired post-deploy via `setIvcVerifier`, or a deploy-script
    ///         regression, still cannot settle reserved escrow while this is off.
    bool public ivcEnabled;

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
    event IvcEnabledSet(bool enabled);

    modifier onlyOperator() {
        require(operators[msg.sender], "not operator");
        _;
    }

    /// @dev Gates the IVC settlement path (submitSession + settleAuction) on the
    ///      owner having explicitly armed it via setIvcEnabled. See `ivcEnabled`.
    modifier whenIvcEnabled() {
        require(ivcEnabled, "ivc settlement disabled");
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

    /// @notice Lock free balance for trading. Only reserved funds are eligible
    ///         for matching and settlement (see `reserved`). Moves `amount`
    ///         from withdrawable `balances` into the settlement-locked
    ///         `reserved` bucket. No external calls, so no reentrancy surface.
    function reserve(address token, uint256 amount) external whenNotPaused {
        require(amount > 0, "zero amount");
        require(balances[msg.sender][token] >= amount, "insufficient balance");
        balances[msg.sender][token] -= amount;
        reserved[msg.sender][token] += amount;
        emit Reserved(msg.sender, token, amount);
    }

    /// @notice Begin unlocking reserved funds. The funds STAY in `reserved`
    ///         (and remain claimable by settlement) until `releaseUnreserve`
    ///         is called after `UNRESERVE_DELAY`. That delay is what closes the
    ///         #165 griefing vector: a matched trader cannot pull committed
    ///         funds within the window between matching and settlement.
    /// @dev A second request before the first matures extends the deadline for
    ///      the whole pending bucket — deliberately conservative (it can only
    ///      lock funds longer, never release them early).
    function requestUnreserve(address token, uint256 amount) external whenNotPaused {
        require(amount > 0, "zero amount");
        uint256 r = reserved[msg.sender][token];
        uint256 p = pendingUnreserve[msg.sender][token];
        uint256 available = r > p ? r - p : 0;
        require(amount <= available, "exceeds reserved");
        pendingUnreserve[msg.sender][token] = p + amount;
        uint256 readyAt = block.timestamp + UNRESERVE_DELAY;
        unreserveReadyAt[msg.sender][token] = readyAt;
        emit UnreserveRequested(msg.sender, token, amount, readyAt);
    }

    /// @notice Move matured unreserve funds from `reserved` back to
    ///         withdrawable `balances`. Releases the lesser of the pending
    ///         amount and what remains reserved: settlement may have
    ///         legitimately consumed some pending funds (a matched trade wins
    ///         over a pending unbond), so clamping to `reserved` avoids
    ///         underflow and clears the stale request either way.
    function releaseUnreserve(address token) external whenNotPaused {
        uint256 pending = pendingUnreserve[msg.sender][token];
        require(pending > 0, "nothing pending");
        // UNRESERVE_DELAY is hour-scale, so the few seconds of timestamp drift a
        // validator could introduce is immaterial to the settlement safety margin.
        // forge-lint: disable-next-line(block-timestamp)
        require(block.timestamp >= unreserveReadyAt[msg.sender][token], "unreserve not ready");
        uint256 r = reserved[msg.sender][token];
        uint256 amount = pending <= r ? pending : r;
        reserved[msg.sender][token] = r - amount;
        balances[msg.sender][token] += amount;
        pendingUnreserve[msg.sender][token] = 0;
        unreserveReadyAt[msg.sender][token] = 0;
        emit Unreserved(msg.sender, token, amount);
    }

    function submitBatch(
        bytes32 batchId,
        bytes32 auctionId,
        bytes calldata proof,
        uint256[6] calldata publicInputs,
        Match[] calldata matches
    ) external onlyOperator whenNotPaused {
        require(!settledBatch[batchId], "batch already settled");
        // Shared with the IVC arm: if this round already settled via
        // submitSession + settleAuction, reject the Groth16 replay (#214).
        require(!settledAuction[auctionId], "auction already settled");
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

        settledBatch[batchId] = true;
        // Mark the economic round settled in the shared namespace so the IVC arm
        // cannot later re-settle the same auctionId (#214).
        settledAuction[auctionId] = true;
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

    /// @dev Debits the settlement-locked `reserved` escrow — never free
    ///      `balances` — for what each side owes; credits proceeds to free
    ///      `balances`. If a trader lacks sufficient *reserved* funds the
    ///      subtraction underflows and the whole batch reverts. That atomic
    ///      revert is the intended fail-safe (#165): settling a financially
    ///      inconsistent subset, or letting the operator "skip" the offending
    ///      match, would break batch-proof atomicity (publicInputs[0] attests
    ///      matches.length) and hand the operator a censorship/selection lever.
    ///      The reserve + cooldown mechanism makes this revert unreachable for
    ///      honest flow, because matched funds cannot be withdrawn before
    ///      settlement. (Operator-side: matching MUST gate on on-chain
    ///      `reserved` so a valid batch never references unreserved funds —
    ///      see the BalanceOracle follow-up, which overlaps #153.)
    function _settleMatch(Match calldata m) internal {
        // `m.price`/`m.size` arrive in the canonical wei domain (`decimal * 1e18`,
        // per the #209 binding — `_settlementChain` divides them by
        // WEI_TO_CIRCUIT_SCALE to recover the proved 1e8 values). Convert each
        // leg here into its token's raw on-chain units using that token's own
        // decimals, so a non-18-decimal quote (6-decimal USDC) settles correctly
        // instead of 10^12x off (#211). Reading decimals() live is the canonical
        // source and is safe: only vetted standard ERC20s are ever allowlisted.
        uint8 baseDecimals = IERC20Metadata(m.baseToken).decimals();
        uint8 quoteDecimals = IERC20Metadata(m.quoteToken).decimals();

        // size: decimal * 1e18  ->  decimal * 10^baseDecimals (base raw).
        uint256 baseAmount = m.size * (uint256(10) ** baseDecimals) / 1e18;
        // notional: `m.price * m.size / 1e18` is the notional still in the 1e18
        // domain; rescale it to the quote token's decimals (quote raw). For an
        // 18-decimal quote this is identity, so existing settlement is unchanged.
        uint256 notional = (m.price * m.size / 1e18) * (uint256(10) ** quoteDecimals) / 1e18;
        uint256 fee = notional * PROTOCOL_FEE_BPS / BPS_DENOMINATOR;
        uint256 askReceives = notional - fee;

        _consumeReserved(m.bidTrader, m.quoteToken, notional);
        balances[m.bidTrader][m.baseToken] += baseAmount;

        _consumeReserved(m.askTrader, m.baseToken, baseAmount);
        balances[m.askTrader][m.quoteToken] += askReceives;

        balances[feeRecipient][m.quoteToken] += fee;
    }

    /// @dev Spend `amount` of a trader's reserved collateral at settlement.
    ///      A matched trade takes priority over a pending unbond, so
    ///      free-reserved funds are consumed first and `pendingUnreserve` only
    ///      shrinks when the match must reach into funds the trader had asked to
    ///      unlock. Retiring the pending request (and clearing
    ///      `unreserveReadyAt` once it hits zero) is what stops a stale unbond
    ///      timer from later releasing freshly reserved collateral with no
    ///      cooldown: otherwise settlement would zero `reserved` while leaving a
    ///      matured timer, and the trader's next `reserve` would be instantly
    ///      releasable — reopening the #165 withdraw-before-settlement vector.
    ///      The `reserved -= amount` keeps its checked-math underflow, which is
    ///      the intended fail-safe that reverts the whole batch when a trader
    ///      lacks sufficient reserved funds.
    function _consumeReserved(address trader, address token, uint256 amount) internal {
        uint256 r = reserved[trader][token];
        reserved[trader][token] = r - amount;

        uint256 p = pendingUnreserve[trader][token];
        if (p != 0) {
            uint256 freeReserved = r > p ? r - p : 0;
            if (amount > freeReserved) {
                uint256 newPending = p - (amount - freeReserved);
                pendingUnreserve[trader][token] = newPending;
                if (newPending == 0) {
                    unreserveReadyAt[trader][token] = 0;
                }
            }
        }
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

    /// @notice Arm or disarm the IVC settlement path. Off until the owner turns
    ///         it on, which should happen only after a real Decider verifier
    ///         (the trusted-setup output, not the all-accepting stub) is wired
    ///         via setIvcVerifier. Kept independent of the verifier pointer on
    ///         purpose: settlement needs both a verifier set AND the path armed,
    ///         so neither a stray setIvcVerifier nor a stale arm settles alone.
    function setIvcEnabled(bool enabled) external onlyOwner {
        ivcEnabled = enabled;
        emit IvcEnabledSet(enabled);
    }

    /// @notice Commit to an IVC-proved session. The matches are bound to the
    ///         proof by its settlement hash-chain `zN[3]` (#209), which
    ///         settleAuction recomputes from `matches[]` — no separate
    ///         operator-supplied matches commitment is taken.
    /// @param z0 IVC initial state (5 elements).
    /// @param zN IVC final state (5 elements); `zN[3]` is the settlement
    ///        accumulator the proof carries over the matched tuples.
    function submitSession(
        bytes32 sessionId,
        bytes calldata proof,
        uint256[5] calldata z0,
        uint256[5] calldata zN,
        uint64 nSteps,
        bytes32 policyHash
    ) external onlyOperator whenNotPaused whenIvcEnabled {
        require(!sessionSubmitted[sessionId], "session already submitted");
        require(address(ivcVerifier) != address(0), "ivc verifier not set");
        require(ivcVerifier.verifyIvcProof(proof, z0, zN, nSteps), "invalid ivc proof");
        // Defense in depth on the proof's public IO. Neither check stops an
        // escrow drain on its own: a non-zero z0[3] only makes settleAuction's
        // chain (which starts at 0) impossible to match, and the policy is bound
        // in-circuit. They pin the session to its canonical form — rejecting a
        // malformed z0 up front and binding the emitted policyHash to the proved
        // zN[2] so the event is trustworthy. The stub decider ignores zN, so
        // today these guard operator mistakes; the real decider (#210) is what
        // makes zN attacker-infeasible.
        require(z0[3] == 0, "z0 settlement acc not zero");
        require(uint256(policyHash) == zN[2], "policy hash mismatch");
        sessionSubmitted[sessionId] = true;
        sessionSettlementAcc[sessionId] = zN[3];
        emit SessionSubmitted(sessionId, nSteps, policyHash);
    }

    /// @notice Settle an IVC-proved auction by replaying its matches. The
    ///         settled `matches[]` are bound to the proof (#209): this recomputes
    ///         the Poseidon settlement hash-chain
    ///         `acc = poseidon(acc, bidTrader, askTrader, price_1e8, size_1e8)`
    ///         over the array in order and requires it equals the session's
    ///         committed `zN[3]`. Any substituted, reordered, or repriced match
    ///         changes the accumulator and reverts, so a passing proof can only
    ///         settle the exact matches it proved.
    /// @dev    Enforcement is complete only once the stub decider is replaced
    ///         (#210): the stub ignores `zN`, leaving it operator-controlled.
    ///         The recompute and `zN[3]` plumbing are in place so that swap
    ///         activates the binding with no further contract change.
    function settleAuction(
        bytes32 sessionId,
        bytes32 auctionId,
        IDarkPool.Match[] calldata matches
    ) external onlyOperator whenNotPaused whenIvcEnabled {
        require(sessionSubmitted[sessionId], "session not submitted");
        // Shared with the Groth16 arm (#214): blocks re-settling this round
        // whether it first settled via this path or via submitBatch.
        require(!settledAuction[auctionId], "auction already settled");
        // A session proves exactly one auction's matches. Because the binding
        // below is over `matches[]` only (auctionId is absent from the proof's
        // hash-chain), the same proved session would otherwise settle multiple
        // distinct auctionIds with the same matches — replaying `_settleMatch`
        // and double-spending reserved escrow. Bind each session to a single
        // settlement.
        require(!sessionSettled[sessionId], "session already settled");
        // Defense in depth: once an auction is bound to a session, only that
        // session can settle it. `settledAuction[auctionId]` already blocks
        // re-settlement; this guards a cross-session race where two distinct
        // submitSession calls both try to settle the same auctionId.
        bytes32 bound = auctionToSession[auctionId];
        require(bound == bytes32(0) || bound == sessionId, "auction bound to other session");
        // Tighter than the Groth16 `MAX_BATCH_SIZE`: the on-chain Poseidon
        // recompute below makes a 256-match settle exceed the block gas limit
        // (see `MAX_IVC_SETTLE_MATCHES`). Fail fast here rather than let the
        // operator broadcast a settle tx that can never be mined.
        require(matches.length > 0 && matches.length <= MAX_IVC_SETTLE_MATCHES, "invalid ivc batch size");
        // Bind the settled matches to the proof: recompute the Poseidon chain
        // over `matches[]` and require it equals the proved `zN[3]`.
        require(_settlementChain(matches) == sessionSettlementAcc[sessionId], "settlement binding");
        auctionToSession[auctionId] = sessionId;
        settledAuction[auctionId] = true;
        sessionSettled[sessionId] = true;
        for (uint256 i = 0; i < matches.length; i++) {
            _settleMatch(matches[i]);
        }
        emit AuctionSettled(sessionId, auctionId);
    }

    /// @dev Recompute the #209 settlement hash-chain over `matches[]` in array
    ///      order — the on-chain mirror of `dp_zk::settlement_chain` and the
    ///      in-circuit `settlement_acc`. Each match folds in as
    ///      `poseidon(acc, uint256(bidTrader), uint256(askTrader), price_1e8,
    ///      size_1e8)`. `price`/`size` are scaled down from the wei domain
    ///      (`decimal * 1e18`) to the circuit's 1e8 domain; a value carrying
    ///      sub-1e8 precision (not divisible by WEI_TO_CIRCUIT_SCALE) could not
    ///      have been proved, so it is rejected before it reaches the chain.
    function _settlementChain(IDarkPool.Match[] calldata matches) internal pure returns (uint256) {
        // Build the Poseidon constants once for the whole chain rather than
        // reconstructing them on every fold — that reconstruction is the
        // dominant per-match cost and a batch carries up to MAX_IVC_SETTLE_MATCHES.
        (uint256[3][65] memory ark, uint256[3][3] memory mds) = PoseidonBN254.loadConstants();
        uint256 acc = 0;
        for (uint256 i = 0; i < matches.length; i++) {
            IDarkPool.Match calldata m = matches[i];
            require(m.price % WEI_TO_CIRCUIT_SCALE == 0 && m.size % WEI_TO_CIRCUIT_SCALE == 0, "match precision");
            acc = PoseidonBN254.poseidon5(
                ark,
                mds,
                acc,
                uint256(uint160(m.bidTrader)),
                uint256(uint160(m.askTrader)),
                m.price / WEI_TO_CIRCUIT_SCALE,
                m.size / WEI_TO_CIRCUIT_SCALE
            );
        }
        return acc;
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
