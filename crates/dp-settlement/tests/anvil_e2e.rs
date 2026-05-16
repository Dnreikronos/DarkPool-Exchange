use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;
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
    BatchSink, DarkPool, EthSubmitter, EthSubmitterConfig, SettlementError, SettlementMatch,
    SubmitBatchParams, Submitter, Watcher,
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

fn read_bytecode(rel_dir: &str, contract: &str) -> Option<Vec<u8>> {
    let path = workspace_root()
        .join("contracts")
        .join("out")
        .join(rel_dir)
        .join(format!("{contract}.json"));
    let content = std::fs::read_to_string(&path).ok()?;
    let v: Value = serde_json::from_str(&content).ok()?;
    let hex_str = v.get("bytecode")?.get("object")?.as_str()?;
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(hex_str).ok()
}

macro_rules! await_tx {
    ($call:expr) => {
        $call.send().await.unwrap().get_receipt().await.unwrap()
    };
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

    let Some(mock_verifier_bc) = read_bytecode("DarkPool.t.sol", "MockVerifier") else {
        eprintln!(
            "skip anvil_e2e: MockVerifier artifact missing (run `cd contracts && forge build`)"
        );
        return;
    };
    let Some(erc20_bc) = read_bytecode("MockERC20.sol", "MockERC20") else {
        eprintln!("skip anvil_e2e: MockERC20 artifact missing");
        return;
    };
    let Some(pool_bc) = read_bytecode("DarkPool.sol", "DarkPool") else {
        eprintln!("skip anvil_e2e: DarkPool artifact missing");
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

    // Deploy DarkPool(verifier, feeRecipient = signer_addr)
    let mut pool_code = pool_bc;
    pool_code.extend_from_slice(&(verifier_addr, signer_addr).abi_encode_params());
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
    await_tx!(pool.deposit(base_addr, size));
    await_tx!(pool.deposit(quote_addr, notional));
    await_tx!(pool.addOperator(signer_addr));

    // Spawn watcher and await its subscribe_logs handshake before submitting.
    let (sink, batch_rx) = BatchCollector::new();
    let cancel = CancellationToken::new();
    let (ready_tx, ready_rx) = oneshot::channel();
    let watcher = Watcher::new(ws_provider, pool_addr, sink, cancel.clone())
        .with_ready_signal(ready_tx);
    let watcher_handle = tokio::spawn(async move { watcher.run().await });

    tokio::time::timeout(Duration::from_secs(5), ready_rx)
        .await
        .expect("watcher subscribe_logs timeout")
        .expect("watcher dropped ready signal");

    // Submit a batch via EthSubmitter
    let config = EthSubmitterConfig {
        rpc_url: anvil.endpoint(),
        private_key: ANVIL_KEY_0_HEX.to_string(),
        contract_address: pool_addr.to_string(),
        chain_id: ANVIL_CHAIN_ID,
        gas_limit: Some(2_000_000),
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
