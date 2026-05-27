use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root().join("..").join("..")
}

fn check(contract: &str, artifact_sol_dir: &str) {
    let snapshot_path = crate_root().join("abi").join(format!("{contract}.json"));
    let artifact_path = workspace_root()
        .join("contracts")
        .join("out")
        .join(artifact_sol_dir)
        .join(format!("{contract}.json"));

    if !artifact_path.exists() {
        eprintln!(
            "skip {contract} drift check: artifact missing at {} (run `cd contracts && forge build`)",
            artifact_path.display()
        );
        return;
    }

    let snapshot: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&snapshot_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", snapshot_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", snapshot_path.display()));

    let artifact: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&artifact_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", artifact_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", artifact_path.display()));

    let live_abi = artifact
        .get("abi")
        .unwrap_or_else(|| panic!("{}: missing .abi field", artifact_path.display()));

    if sort_abi_top_level(snapshot) != sort_abi_top_level(live_abi.clone()) {
        panic!(
            "{contract} ABI drift detected.\n  Snapshot: {}\n  Artifact: {}\nRegenerate:\n\
             jq '.abi' {} > {}",
            snapshot_path.display(),
            artifact_path.display(),
            relative_from_workspace(&artifact_path),
            relative_from_workspace(&snapshot_path),
        );
    }
}

// Sort top-level ABI entries so a Foundry re-emit that only reorders items
// does not register as drift. Nested arrays (inputs/outputs/components) keep
// their order — parameter position is semantically meaningful.
fn sort_abi_top_level(v: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Array(mut arr) = v else {
        return v;
    };
    arr.sort_by_key(abi_entry_key);
    serde_json::Value::Array(arr)
}

fn abi_entry_key(v: &serde_json::Value) -> String {
    let t = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    let n = v.get("name").and_then(|x| x.as_str()).unwrap_or("");
    // Include input type signature so overloaded functions stay distinct.
    let inputs = v
        .get("inputs")
        .map(|i| serde_json::to_string(i).unwrap_or_default())
        .unwrap_or_default();
    format!("{t}\u{1f}{n}\u{1f}{inputs}")
}

fn relative_from_workspace(p: &Path) -> String {
    p.strip_prefix(workspace_root())
        .map(|r| r.display().to_string())
        .unwrap_or_else(|_| p.display().to_string())
}

#[test]
fn darkpool_abi_matches_artifact() {
    check("DarkPool", "DarkPool.sol");
}

#[test]
fn verifier_proxy_abi_matches_artifact() {
    check("VerifierProxy", "VerifierProxy.sol");
}
