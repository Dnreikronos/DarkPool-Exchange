//! Convert a serialized arkworks VK into Solidity-friendly JSON
//! (constructor args for `Groth16Verifier.sol`).

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "dp-zk-vk-export",
    about = "Export arkworks VK to Solidity JSON"
)]
struct Args {
    #[arg(long)]
    keys_dir: PathBuf,
    #[arg(long)]
    out: Option<PathBuf>,
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let vk = dp_zk::keys::read_vk(&args.keys_dir)?;
    let sol = dp_zk::keys::vk_to_solidity(&vk);
    let json = serde_json::to_string_pretty(&sol)?;

    match args.out {
        Some(path) => {
            fs::write(&path, json.as_bytes())?;
            eprintln!("wrote {} ({} ic entries)", path.display(), sol.ic.len());
        }
        None => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(json.as_bytes())?;
            let _ = stdout.write_all(b"\n");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
