//! Convert a serialized arkworks VK into Solidity-friendly JSON
//! (constructor args for `Groth16Verifier.sol`).

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "dp-zk-vk-export", about = "Export arkworks VK to Solidity JSON")]
struct Args {
    /// Directory containing `verifying_key.bin` (and `keys_metadata.json`).
    #[arg(long)]
    keys_dir: PathBuf,
    /// Output JSON path. Omit for stdout.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let vk = match dp_zk::keys::read_vk(&args.keys_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("read_vk: {e}");
            return ExitCode::from(2);
        }
    };

    let sol = dp_zk::keys::vk_to_solidity(&vk);
    let json = match serde_json::to_string_pretty(&sol) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("serialize: {e}");
            return ExitCode::from(2);
        }
    };

    match args.out {
        Some(path) => {
            if let Err(e) = fs::write(&path, json.as_bytes()) {
                eprintln!("write {}: {e}", path.display());
                return ExitCode::from(2);
            }
            eprintln!("wrote {} ({} ic entries)", path.display(), sol.ic.len());
        }
        None => {
            let mut stdout = io::stdout().lock();
            if let Err(e) = stdout.write_all(json.as_bytes()) {
                eprintln!("stdout: {e}");
                return ExitCode::from(2);
            }
            let _ = stdout.write_all(b"\n");
        }
    }

    ExitCode::SUCCESS
}
