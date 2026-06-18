//! `dp-trade` — construct an encrypted order submission and either print it
//! or POST it to a running darkpool node.

use std::path::PathBuf;
use std::process::ExitCode;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use clap::{Parser, ValueEnum};
use rand::RngCore;
use rust_decimal::Decimal;

use dp_client::encrypt::{load_operator_pubkey_file, parse_operator_pubkey_hex};
use dp_client::payload::{OrderPayload, Side};
use dp_client::{prepare_order, ClientError, OrderSubmission};

#[derive(Copy, Clone, Debug, ValueEnum)]
enum SideArg {
    Buy,
    Sell,
}

impl From<SideArg> for Side {
    fn from(s: SideArg) -> Self {
        match s {
            SideArg::Buy => Side::Buy,
            SideArg::Sell => Side::Sell,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum OutputFormat {
    Json,
    Hex,
    Base64,
}

#[derive(Parser, Debug)]
#[command(
    name = "dp-trade",
    about = "Build an encrypted DarkPool order submission"
)]
struct Args {
    /// Trader address bound into the encrypted payload.
    #[arg(long)]
    trader: String,

    #[arg(long)]
    pair: String,

    #[arg(long, value_enum)]
    side: SideArg,

    #[arg(long)]
    price: Decimal,

    #[arg(long)]
    size: Decimal,

    #[arg(long)]
    commitment_key: String,

    /// Order TTL (humantime, e.g. "10m", "30s"). Stored as nanoseconds.
    #[arg(long, default_value = "10m")]
    ttl: String,

    /// SEC1 secp256k1 hex (0x-prefixed or bare) OR path to a file containing one.
    #[arg(long)]
    operator_key: String,

    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,

    /// If set, POST base64-encoded submission to <server>/v1/orders.
    #[arg(long)]
    server: Option<String>,

    /// API key, sent as `x-api-key` when --server is used.
    #[arg(long)]
    api_key: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), ClientError> {
    let args = Args::parse();

    let operator_pubkey = resolve_operator_key(&args.operator_key)?;

    let ttl = humantime::parse_duration(&args.ttl)
        .map_err(|e| ClientError::InvalidPayload(format!("--ttl: {e}")))?;
    let ttl_nanos: i64 = ttl
        .as_nanos()
        .try_into()
        .map_err(|_| ClientError::InvalidPayload("ttl exceeds i64 nanoseconds".into()))?;

    let payload = OrderPayload {
        trader: args.trader,
        pair: args.pair,
        side: args.side.into(),
        price: args.price,
        size: args.size,
        commitment_key: args.commitment_key,
        salt: random_salt_hex(),
        ttl: ttl_nanos,
    };

    let sub = prepare_order(&operator_pubkey, &payload)?;

    print_submission(&sub, args.output);

    if let Some(server) = args.server {
        eprintln!(
            "warning: dp-trade sends a development placeholder proof; production servers reject it unless DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS=true"
        );
        submit_http(&server, args.api_key.as_deref(), &sub).await?;
    }

    Ok(())
}

fn random_salt_hex() -> String {
    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);
    hex::encode(salt)
}

fn resolve_operator_key(input: &str) -> Result<Vec<u8>, ClientError> {
    let p = PathBuf::from(input);
    if p.exists() {
        load_operator_pubkey_file(&p)
    } else {
        parse_operator_pubkey_hex(input)
    }
}

fn print_submission(sub: &OrderSubmission, fmt: OutputFormat) {
    match fmt {
        OutputFormat::Json => {
            let v = serde_json::json!({
                "commitment": hex::encode(sub.commitment),
                "proof": hex::encode(&sub.proof),
                "encrypted_payload": B64.encode(&sub.encrypted_payload),
                "trader_id": hex::encode(sub.trader_id),
                "salt": hex::encode(sub.salt),
            });
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        OutputFormat::Hex => {
            println!("commitment={}", hex::encode(sub.commitment));
            println!("proof={}", hex::encode(&sub.proof));
            println!("encrypted_payload={}", hex::encode(&sub.encrypted_payload));
            println!("trader_id={}", hex::encode(sub.trader_id));
            println!("salt={}", hex::encode(sub.salt));
        }
        OutputFormat::Base64 => {
            println!("commitment={}", B64.encode(sub.commitment));
            println!("proof={}", B64.encode(&sub.proof));
            println!("encrypted_payload={}", B64.encode(&sub.encrypted_payload));
            println!("trader_id={}", B64.encode(sub.trader_id));
            println!("salt={}", B64.encode(sub.salt));
        }
    }
}

async fn submit_http(
    server: &str,
    api_key: Option<&str>,
    sub: &OrderSubmission,
) -> Result<(), ClientError> {
    let url = format!("{}/v1/orders", server.trim_end_matches('/'));
    let body = serde_json::json!({
        "commitment": B64.encode(sub.commitment),
        "proof": B64.encode(&sub.proof),
        "encryptedPayload": B64.encode(&sub.encrypted_payload),
    });

    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&body);
    if let Some(k) = api_key {
        req = req.header("x-api-key", k);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ClientError::Http(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| ClientError::Http(e.to_string()))?;
    if !status.is_success() {
        return Err(ClientError::Http(format!("{status}: {text}")));
    }
    eprintln!("submitted to {url}: {status}");
    if !text.is_empty() {
        eprintln!("{text}");
    }
    Ok(())
}
