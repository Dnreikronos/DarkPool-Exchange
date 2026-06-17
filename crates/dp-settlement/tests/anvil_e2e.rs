use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alloy_network::{EthereumWallet, TransactionBuilder};
use alloy_node_bindings::Anvil;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolValue};
use rust_decimal::Decimal;
use serde_json::Value;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use dp_settlement::{
    BatchSink, DarkPool, EthSubmitter, EthSubmitterConfig, LocalTxSigner, SettlementError,
    SettlementMatch, SettlementTxTransport, SubmitBatchParams, Submitter, TxSigner, Watcher,
};

const ANVIL_KEY_0_HEX: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ANVIL_CHAIN_ID: u64 = 31337;

sol! {
    #[sol(rpc)]
    interface MockERC20Iface {
        function mint(address to, uint256 amount) external;
        function approve(address spender, uint256 amount) external returns (bool);
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

// Returns:
//   Ok(Some(bytes)) → artifact present and parsed.
//   Ok(None)        → artifact file absent (legitimate skip).
//   Err(msg)        → file present but corrupt/invalid (must fail loudly).
fn read_bytecode(rel_dir: &str, contract: &str) -> Result<Option<Vec<u8>>, String> {
    let path = workspace_root()
        .join("contracts")
        .join("out")
        .join(rel_dir)
        .join(format!("{contract}.json"));
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let v: Value =
        serde_json::from_str(&content).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let hex_str = v
        .get("bytecode")
        .and_then(|b| b.get("object"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{}: missing bytecode.object", path.display()))?;
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let bytes = hex::decode(hex_str).map_err(|e| format!("decode {}: {e}", path.display()))?;
    Ok(Some(bytes))
}

fn load_or_skip(rel_dir: &str, contract: &str) -> Option<Vec<u8>> {
    match read_bytecode(rel_dir, contract) {
        Ok(Some(bc)) => Some(bc),
        Ok(None) => {
            eprintln!(
                "skip anvil_e2e: {contract} artifact missing (run `cd contracts && forge build`)"
            );
            None
        }
        Err(msg) => panic!("anvil_e2e: {contract} artifact invalid: {msg}"),
    }
}

macro_rules! await_tx {
    ($call:expr) => {{
        let receipt = $call.send().await.unwrap().get_receipt().await.unwrap();
        alloy_network::ReceiptResponse::ensure_success(&receipt).unwrap();
        receipt
    }};
}

struct BatchCollector {
    inner: Mutex<Option<oneshot::Sender<Uuid>>>,
}

impl BatchCollector {
    fn new() -> (Self, oneshot::Receiver<Uuid>) {
        let (tx, rx) = oneshot::channel();
        (
            Self {
                inner: Mutex::new(Some(tx)),
            },
            rx,
        )
    }
}

impl BatchSink for BatchCollector {
    fn on_batch_settled<'a>(
        &'a self,
        batch_id: Uuid,
        _block_number: u64,
        _tx_hash: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), SettlementError>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(sender) = self.inner.lock().unwrap().take() {
                let _ = sender.send(batch_id);
            }
            Ok(())
        })
    }
}

async fn deploy<P: Provider>(provider: &P, code: Vec<u8>) -> Address {
    let tx = TransactionRequest::default().with_deploy_code(code);
    let receipt = provider
        .send_transaction(tx)
        .await
        .expect("deploy tx")
        .get_receipt()
        .await
        .expect("deploy receipt");
    receipt.contract_address.expect("contract_address")
}

// Run: cargo test -p dp-settlement --test anvil_e2e -- --ignored
// Requires `anvil` on PATH and a prior `cd contracts && forge build`.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn settles_batch_end_to_end() {
    if which::which("anvil").is_err() {
        eprintln!("skip anvil_e2e: anvil not on PATH");
        return;
    }

    let Some(mock_verifier_bc) = load_or_skip("DarkPool.t.sol", "MockVerifier") else {
        return;
    };
    let Some(erc20_bc) = load_or_skip("MockERC20.sol", "MockERC20") else {
        return;
    };
    let Some(pool_bc) = load_or_skip("DarkPool.sol", "DarkPool") else {
        return;
    };

    let anvil = Anvil::new()
        .chain_id(ANVIL_CHAIN_ID)
        .try_spawn()
        .expect("anvil spawn");

    let signer: PrivateKeySigner = ANVIL_KEY_0_HEX.parse().expect("parse signer");
    let signer_addr = signer.address();
    let wallet = EthereumWallet::from(signer);

    let http_provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect(&anvil.endpoint())
        .await
        .expect("http connect");
    let ws_provider = ProviderBuilder::new()
        .connect(&anvil.ws_endpoint())
        .await
        .expect("ws connect");

    // Deploy MockVerifier(true)
    let mut mv_code = mock_verifier_bc;
    mv_code.extend_from_slice(&(true,).abi_encode_params());
    let verifier_addr = deploy(&http_provider, mv_code).await;

    // Deploy MockERC20 base + quote
    let decimals = U256::from(18u8);
    let base_addr = {
        let mut code = erc20_bc.clone();
        code.extend_from_slice(
            &("Base".to_string(), "BASE".to_string(), decimals).abi_encode_params(),
        );
        deploy(&http_provider, code).await
    };
    let quote_addr = {
        let mut code = erc20_bc.clone();
        code.extend_from_slice(
            &("Quote".to_string(), "QUOTE".to_string(), decimals).abi_encode_params(),
        );
        deploy(&http_provider, code).await
    };

    // Deploy DarkPool(verifier, feeRecipient = signer_addr, operatorPubkey).
    // The pubkey is the SEC1-compressed encoding of secp256k1's
    // generator — a 33-byte placeholder is enough to satisfy the
    // constructor's length check for this end-to-end smoke test;
    // ciphertext decryption is not exercised here.
    let placeholder_pubkey: alloy_primitives::Bytes = alloy_primitives::Bytes::from_static(&[
        0x02, 0x79, 0xBE, 0x66, 0x7E, 0xF9, 0xDC, 0xBB, 0xAC, 0x55, 0xA0, 0x62, 0x95, 0xCE, 0x87,
        0x0B, 0x07, 0x02, 0x9B, 0xFC, 0xDB, 0x2D, 0xCE, 0x28, 0xD9, 0x59, 0xF2, 0x81, 0x5B, 0x16,
        0xF8, 0x17, 0x98,
    ]);
    let mut pool_code = pool_bc;
    pool_code
        .extend_from_slice(&(verifier_addr, signer_addr, placeholder_pubkey).abi_encode_params());
    let pool_addr = deploy(&http_provider, pool_code).await;

    // Mint + approve + deposit
    let one_e18 = U256::from(1_000_000_000_000_000_000u128);
    let size = U256::from(10) * one_e18; // 10 base
    let notional = U256::from(1000) * one_e18; // price=100 * size=10

    let base = MockERC20Iface::new(base_addr, &http_provider);
    let quote = MockERC20Iface::new(quote_addr, &http_provider);
    await_tx!(base.mint(signer_addr, size));
    await_tx!(quote.mint(signer_addr, notional));
    await_tx!(base.approve(pool_addr, size));
    await_tx!(quote.approve(pool_addr, notional));

    let pool = DarkPool::new(pool_addr, &http_provider);
    // deposit() rejects tokens not on the allowlist; allowlist the pair first.
    await_tx!(pool.setTokenAllowed(base_addr, true));
    await_tx!(pool.setTokenAllowed(quote_addr, true));
    await_tx!(pool.deposit(base_addr, size));
    await_tx!(pool.deposit(quote_addr, notional));
    // Settlement debits the locked `reserved` escrow (#165), so matched funds
    // must be reserved out of free balance before the batch can settle.
    // signer_addr is both bid and ask here: reserve quote for the bid leg and
    // base for the ask leg.
    await_tx!(pool.reserve(quote_addr, notional));
    await_tx!(pool.reserve(base_addr, size));
    await_tx!(pool.addOperator(signer_addr));

    // Spawn watcher and await its subscribe_logs handshake before submitting.
    let (sink, batch_rx) = BatchCollector::new();
    let cancel = CancellationToken::new();
    let (ready_tx, ready_rx) = oneshot::channel();
    let watcher =
        Watcher::new(ws_provider, pool_addr, sink, cancel.clone()).with_ready_signal(ready_tx);
    let watcher_handle = tokio::spawn(async move { watcher.run().await });

    tokio::time::timeout(Duration::from_secs(5), ready_rx)
        .await
        .expect("watcher subscribe_logs timeout")
        .expect("watcher dropped ready signal");

    // Submit a batch via EthSubmitter. Build the signer from the same
    // hex used to seed anvil so the wallet address matches the operator
    // registered above.
    let tx_signer: Arc<dyn TxSigner> =
        Arc::new(LocalTxSigner::from_hex(ANVIL_KEY_0_HEX).expect("local signer"));
    let config = EthSubmitterConfig {
        rpc_url: anvil.endpoint(),
        signer: tx_signer,
        contract_address: pool_addr.to_string(),
        chain_id: ANVIL_CHAIN_ID,
        gas_limit: Some(2_000_000),
        tx_transport: SettlementTxTransport::PublicMempool,
    };
    let submitter = EthSubmitter::new(http_provider, &config).expect("submitter");

    let batch_id = Uuid::new_v4();
    let params = SubmitBatchParams {
        batch_id,
        auction_id: Uuid::new_v4(),
        proof: vec![0u8; 256],
        public_inputs: [
            U256::from(1),
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
        ],
        matches: vec![SettlementMatch {
            bid_order_id: Uuid::new_v4(),
            ask_order_id: Uuid::new_v4(),
            bid_trader: signer_addr,
            ask_trader: signer_addr,
            base_token: base_addr,
            quote_token: quote_addr,
            price: Decimal::from(100),
            size: Decimal::from(10),
        }],
    };

    let tx_hash = submitter.submit(&params).await.expect("submit");
    assert!(tx_hash.starts_with("0x"));

    let received = tokio::time::timeout(Duration::from_secs(5), batch_rx)
        .await
        .expect("BatchSettled timeout")
        .expect("watcher oneshot dropped");
    assert_eq!(received, batch_id);

    cancel.cancel();
    let _ = watcher_handle.await;
}

// Run: cargo test -p dp-settlement --test anvil_e2e -- --ignored
// Requires `anvil` on PATH and a prior `cd contracts && forge build`.
//
// #211 regression. The quote token is deployed with 6 decimals (USDC-like)
// while the base stays 18. The operator submits the SAME canonical wire amounts
// as the 18/18 case (`decimal_to_wei` = decimal * 1e18, unchanged); the contract
// must settle the quote leg in 1e6 units. Pre-fix `_settleMatch` scaled the
// notional by 1e18 (1000e18), underflowing the 1000e6 the bid reserved and
// reverting the whole batch — so `BatchSettled` never fires and the timeout
// below trips. Post-fix the quote settles to exactly 1000e6.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn settles_batch_six_decimal_quote() {
    if which::which("anvil").is_err() {
        eprintln!("skip anvil_e2e: anvil not on PATH");
        return;
    }

    let Some(mock_verifier_bc) = load_or_skip("DarkPool.t.sol", "MockVerifier") else {
        return;
    };
    let Some(erc20_bc) = load_or_skip("MockERC20.sol", "MockERC20") else {
        return;
    };
    let Some(pool_bc) = load_or_skip("DarkPool.sol", "DarkPool") else {
        return;
    };

    let anvil = Anvil::new()
        .chain_id(ANVIL_CHAIN_ID)
        .try_spawn()
        .expect("anvil spawn");

    let signer: PrivateKeySigner = ANVIL_KEY_0_HEX.parse().expect("parse signer");
    let signer_addr = signer.address();
    let wallet = EthereumWallet::from(signer);

    let http_provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect(&anvil.endpoint())
        .await
        .expect("http connect");
    let ws_provider = ProviderBuilder::new()
        .connect(&anvil.ws_endpoint())
        .await
        .expect("ws connect");

    // Deploy MockVerifier(true)
    let mut mv_code = mock_verifier_bc;
    mv_code.extend_from_slice(&(true,).abi_encode_params());
    let verifier_addr = deploy(&http_provider, mv_code).await;

    // Deploy MockERC20 base (18 decimals) + quote (6 decimals, USDC-like).
    let base_addr = {
        let mut code = erc20_bc.clone();
        code.extend_from_slice(
            &("Base".to_string(), "BASE".to_string(), U256::from(18u8)).abi_encode_params(),
        );
        deploy(&http_provider, code).await
    };
    let quote_addr = {
        let mut code = erc20_bc.clone();
        code.extend_from_slice(
            &("Quote".to_string(), "QUOTE".to_string(), U256::from(6u8)).abi_encode_params(),
        );
        deploy(&http_provider, code).await
    };

    let placeholder_pubkey: alloy_primitives::Bytes = alloy_primitives::Bytes::from_static(&[
        0x02, 0x79, 0xBE, 0x66, 0x7E, 0xF9, 0xDC, 0xBB, 0xAC, 0x55, 0xA0, 0x62, 0x95, 0xCE, 0x87,
        0x0B, 0x07, 0x02, 0x9B, 0xFC, 0xDB, 0x2D, 0xCE, 0x28, 0xD9, 0x59, 0xF2, 0x81, 0x5B, 0x16,
        0xF8, 0x17, 0x98,
    ]);
    let mut pool_code = pool_bc;
    pool_code
        .extend_from_slice(&(verifier_addr, signer_addr, placeholder_pubkey).abi_encode_params());
    let pool_addr = deploy(&http_provider, pool_code).await;

    // Canonical wire price=100, size=10. Token-raw amounts: base = 10 * 1e18,
    // quote notional = 100 * 10 * 1e6 (6-decimal USDC), NOT 1e18-scaled.
    let one_e18 = U256::from(1_000_000_000_000_000_000u128);
    let one_e6 = U256::from(1_000_000u64);
    let size = U256::from(10) * one_e18; // 10 base (18 decimals)
    let notional = U256::from(1000) * one_e6; // 1000 quote (6 decimals)

    let base = MockERC20Iface::new(base_addr, &http_provider);
    let quote = MockERC20Iface::new(quote_addr, &http_provider);
    await_tx!(base.mint(signer_addr, size));
    await_tx!(quote.mint(signer_addr, notional));
    await_tx!(base.approve(pool_addr, size));
    await_tx!(quote.approve(pool_addr, notional));

    let pool = DarkPool::new(pool_addr, &http_provider);
    await_tx!(pool.setTokenAllowed(base_addr, true));
    await_tx!(pool.setTokenAllowed(quote_addr, true));
    await_tx!(pool.deposit(base_addr, size));
    await_tx!(pool.deposit(quote_addr, notional));
    // signer_addr is both bid and ask: reserve quote for the bid leg (in 1e6)
    // and base for the ask leg (in 1e18).
    await_tx!(pool.reserve(quote_addr, notional));
    await_tx!(pool.reserve(base_addr, size));
    await_tx!(pool.addOperator(signer_addr));

    let (sink, batch_rx) = BatchCollector::new();
    let cancel = CancellationToken::new();
    let (ready_tx, ready_rx) = oneshot::channel();
    let watcher =
        Watcher::new(ws_provider, pool_addr, sink, cancel.clone()).with_ready_signal(ready_tx);
    let watcher_handle = tokio::spawn(async move { watcher.run().await });

    tokio::time::timeout(Duration::from_secs(5), ready_rx)
        .await
        .expect("watcher subscribe_logs timeout")
        .expect("watcher dropped ready signal");

    let tx_signer: Arc<dyn TxSigner> =
        Arc::new(LocalTxSigner::from_hex(ANVIL_KEY_0_HEX).expect("local signer"));
    let config = EthSubmitterConfig {
        rpc_url: anvil.endpoint(),
        signer: tx_signer,
        contract_address: pool_addr.to_string(),
        chain_id: ANVIL_CHAIN_ID,
        gas_limit: Some(2_000_000),
        tx_transport: SettlementTxTransport::PublicMempool,
    };
    // Clone the provider into the submitter so `pool` (borrowing `http_provider`)
    // stays alive for the post-settlement balance reads below.
    let submitter = EthSubmitter::new(http_provider.clone(), &config).expect("submitter");

    let batch_id = Uuid::new_v4();
    let params = SubmitBatchParams {
        batch_id,
        auction_id: Uuid::new_v4(),
        proof: vec![0u8; 256],
        public_inputs: [
            U256::from(1),
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
        ],
        matches: vec![SettlementMatch {
            bid_order_id: Uuid::new_v4(),
            ask_order_id: Uuid::new_v4(),
            bid_trader: signer_addr,
            ask_trader: signer_addr,
            base_token: base_addr,
            quote_token: quote_addr,
            price: Decimal::from(100),
            size: Decimal::from(10),
        }],
    };

    let tx_hash = submitter.submit(&params).await.expect("submit");
    assert!(tx_hash.starts_with("0x"));

    let received = tokio::time::timeout(Duration::from_secs(5), batch_rx)
        .await
        .expect("BatchSettled timeout — quote leg likely reverted (#211 regression)")
        .expect("watcher oneshot dropped");
    assert_eq!(received, batch_id);

    // The quote leg settled in 6-decimal units, not 1e18. signer_addr is bid,
    // ask, AND feeRecipient, so its quote nets back to the full notional (the
    // fee returns to it) and its base to `size`; reserved is fully consumed.
    let base_bal = pool
        .balances(signer_addr, base_addr)
        .call()
        .await
        .expect("read base balance");
    let quote_bal = pool
        .balances(signer_addr, quote_addr)
        .call()
        .await
        .expect("read quote balance");
    assert_eq!(base_bal, size, "bid received base in 1e18 units");
    assert_eq!(
        quote_bal, notional,
        "quote leg settled in 1e6 units (1000e6), not 1e18"
    );

    let base_reserved = pool
        .reserved(signer_addr, base_addr)
        .call()
        .await
        .expect("read base reserved");
    let quote_reserved = pool
        .reserved(signer_addr, quote_addr)
        .call()
        .await
        .expect("read quote reserved");
    assert_eq!(base_reserved, U256::ZERO, "base escrow fully consumed");
    assert_eq!(quote_reserved, U256::ZERO, "quote escrow fully consumed");

    cancel.cancel();
    let _ = watcher_handle.await;
}
