//! Operator helper for ECIES key rotation.
//!
//! Three modes, one per subcommand:
//!
//! 1. `dp-zk-rotate-key generate --out file:/path/new.hex` — derive a
//!    fresh secp256k1 keypair, persist the secret to the URI, print
//!    the SEC1 (compressed) pubkey to stdout. The pubkey is what the
//!    operator publishes to chain via the contract's
//!    `setOperatorPubkey(bytes,uint64)` call.
//!
//! 2. `dp-zk-rotate-key pubkey --uri file:/path/secret.hex` — load an
//!    existing secret and print its SEC1-compressed pubkey. Useful
//!    when the secret was generated out-of-band.
//!
//! 3. `dp-zk-rotate-key curl-spec --uri file:/path/new.hex
//!    --status rotating` — print the `curl` one-liner that registers
//!    the new key with a running operator via
//!    `POST /v1/admin/keys`. Keeps secret URIs in operator hands;
//!    avoids stuffing them into shell history through autocomplete.
//!
//! On-chain publication is intentionally NOT a CLI mode — the runbook
//! routes through `cast send setOperatorPubkey(...)` so the
//! VK-rotation tx remains a deliberate, auditable human action.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use k256::ecdsa::SigningKey;

#[derive(Parser, Debug)]
#[command(
    name = "dp-zk-rotate-key",
    about = "DarkPool operator ECIES key rotation helper"
)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Generate a fresh ECIES keypair. Writes the 32-byte secp256k1
    /// secret as plaintext hex into the URI's target file (only
    /// `file:` is supported for `generate` — wrap with age externally
    /// for production storage).
    Generate {
        /// Destination URI for the secret. Currently only `file:`.
        #[arg(long)]
        out: String,
        /// Force overwrite of an existing destination file.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Print the SEC1-compressed pubkey for an existing secret.
    /// Convenience for the `publish` step when the operator already
    /// has the secret on disk.
    Pubkey {
        /// `file:`-scheme URI pointing at the 32-byte hex secret.
        #[arg(long)]
        uri: String,
    },
    /// Render the `curl` command an operator runs to register a new
    /// key with the live server's admin endpoint.
    CurlSpec {
        #[arg(long)]
        id: String,
        #[arg(long)]
        uri: String,
        /// One of `active|rotating|sunset`. Defaults to `rotating`.
        #[arg(long, default_value = "rotating")]
        status: String,
        /// Operator key for the `x-api-key` header. The flag exists
        /// to make the printed command explicit; the secret value is
        /// echoed verbatim.
        #[arg(long, default_value = "$OPERATOR_API_KEY")]
        api_key: String,
        /// Server origin (no trailing slash).
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        server: String,
    },
}

fn main() -> ExitCode {
    let args = Args::parse();
    match args.cmd {
        Cmd::Generate { out, force } => match generate(&out, force) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("generate: {e}");
                ExitCode::FAILURE
            }
        },
        Cmd::Pubkey { uri } => match print_pubkey(&uri) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pubkey: {e}");
                ExitCode::FAILURE
            }
        },
        Cmd::CurlSpec {
            id,
            uri,
            status,
            api_key,
            server,
        } => {
            // serde_json escapes quotes/control chars in id/uri/status so
            // an operator passing a URI with a `"` (legal in `?ciphertext=...`
            // query strings, after URL-encoding) does not corrupt the JSON
            // payload pasted into a terminal.
            let body = serde_json::json!({
                "id": id,
                "uri": uri,
                "status": status,
            })
            .to_string();
            // HEREDOC + --data-binary @- sidesteps shell quoting of the
            // body entirely. api_key/server are left as raw text because
            // their defaults (`$OPERATOR_API_KEY`, `http://127.0.0.1:8080`)
            // are themselves shell tokens the operator expects to evaluate.
            println!(
                "curl -sS -X POST -H 'x-api-key: {api_key}' \\\n  -H 'content-type: application/json' \\\n  --data-binary @- \\\n  {server}/v1/admin/keys <<'JSON'\n{body}\nJSON"
            );
            ExitCode::SUCCESS
        }
    }
}

fn generate(uri: &str, force: bool) -> Result<(), String> {
    let path = strip_file_scheme(uri)?;
    if path.exists() && !force {
        return Err(format!(
            "{} already exists; pass --force to overwrite",
            path.display()
        ));
    }
    let sk = SigningKey::random(&mut rand::thread_rng());
    let sk_bytes = sk.to_bytes();
    let pk_compressed = sk.verifying_key().to_sec1_bytes();
    fs::write(&path, hex::encode(sk_bytes))
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    // Tight permissions on POSIX — the secret is the encryption key
    // for every order placed against this operator. On non-POSIX
    // (Windows) skip silently.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    }
    println!("{}", hex::encode(&pk_compressed));
    eprintln!(
        "wrote secret to {} (mode 0600); publish pubkey via `dp-zk-rotate-key pubkey --uri {}` \
         or copy the hex above into `setOperatorPubkey(bytes,uint64)`.",
        path.display(),
        uri,
    );
    Ok(())
}

fn print_pubkey(uri: &str) -> Result<(), String> {
    let path = strip_file_scheme(uri)?;
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let trimmed = raw.trim().trim_start_matches("0x");
    let sk_bytes = hex::decode(trimmed).map_err(|e| format!("decode hex: {e}"))?;
    if sk_bytes.len() != 32 {
        return Err(format!("expected 32-byte secret, got {}", sk_bytes.len()));
    }
    let sk = SigningKey::from_slice(&sk_bytes).map_err(|e| format!("invalid secret: {e}"))?;
    let pk = sk.verifying_key().to_sec1_bytes();
    println!("{}", hex::encode(pk));
    Ok(())
}

fn strip_file_scheme(uri: &str) -> Result<PathBuf, String> {
    uri.strip_prefix("file:")
        .map(PathBuf::from)
        .ok_or_else(|| format!("only file: URIs supported in this subcommand (got {uri})"))
}
