use std::time::Duration;

use clap::Parser;
use dp_api::config::Config;
use serial_test::serial;

fn clear_env() {
    for k in [
        "DARKPOOL_GRPC_ADDR",
        "DARKPOOL_HTTP_ADDR",
        "DARKPOOL_AUCTION_INTERVAL",
        "DARKPOOL_API_KEYS",
        "DARKPOOL_OPERATOR_API_KEYS",
        "DARKPOOL_PAIR_SEED_JSON",
        "DARKPOOL_RATE_LIMIT",
        "DARKPOOL_RATE_BURST",
        "DARKPOOL_RATE_STALE_AFTER",
        "DARKPOOL_EVENT_LOG",
        "DARKPOOL_EVENT_DB",
        "DARKPOOL_OPERATOR_KEY",
        "DARKPOOL_OPERATOR_KEY_URIS",
        "DARKPOOL_SIGNER_KEY_URI",
        "DARKPOOL_AGGREGATOR_BIN",
        "DARKPOOL_AGGREGATOR_TIMEOUT",
        "DARKPOOL_ZK_PROVING_KEY",
        "DARKPOOL_ZK_BATCH_SIZE",
        "DARKPOOL_SUBMIT_TIMEOUT",
        "DARKPOOL_ETH_RPC",
        "DARKPOOL_SETTLEMENT_PRIVATE_RPC",
        "DARKPOOL_ALLOW_PUBLIC_SETTLEMENT",
        "DARKPOOL_CONTRACT_ADDR",
        "DARKPOOL_CHAIN_ID",
        "DARKPOOL_SUBMIT_GAS",
    ] {
        std::env::remove_var(k);
    }
}

#[test]
#[serial]
fn defaults() {
    clear_env();
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(cfg.grpc_addr.to_string(), "0.0.0.0:9090");
    assert_eq!(cfg.http_addr.to_string(), "0.0.0.0:8080");
    assert_eq!(cfg.auction_interval, Duration::from_secs(5));
    assert!(cfg.api_keys().is_empty());
    assert_eq!(cfg.rate_limit, 10.0);
    assert_eq!(cfg.rate_burst, 20.0);
    assert_eq!(cfg.rate_stale_after, Duration::from_secs(600));
    assert_eq!(cfg.submit_gas, 500_000);
}

#[test]
#[serial]
fn env_fills_defaults() {
    clear_env();
    std::env::set_var("DARKPOOL_GRPC_ADDR", "127.0.0.1:1111");
    std::env::set_var("DARKPOOL_RATE_LIMIT", "42.5");
    std::env::set_var("DARKPOOL_API_KEYS", "k1,k2,k3");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(cfg.grpc_addr.to_string(), "127.0.0.1:1111");
    assert_eq!(cfg.rate_limit, 42.5);
    assert_eq!(cfg.api_keys(), vec!["k1", "k2", "k3"]);
    clear_env();
}

#[test]
#[serial]
fn flag_overrides_env() {
    clear_env();
    std::env::set_var("DARKPOOL_GRPC_ADDR", "127.0.0.1:1111");
    let cfg = Config::try_parse_from(["bin", "--grpc-addr", "127.0.0.1:2222"]).unwrap();
    assert_eq!(cfg.grpc_addr.to_string(), "127.0.0.1:2222");
    clear_env();
}

#[test]
#[serial]
fn parses_humantime_duration() {
    clear_env();
    let cfg = Config::try_parse_from(["bin", "--auction-interval", "2m30s"]).unwrap();
    assert_eq!(cfg.auction_interval, Duration::from_secs(150));
}

#[test]
#[serial]
fn empty_api_keys_string_yields_empty_vec() {
    clear_env();
    std::env::set_var("DARKPOOL_API_KEYS", "");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.api_keys().is_empty());
    clear_env();
}

#[test]
#[serial]
fn optional_accessors_empty_by_default() {
    clear_env();
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.event_db_url().is_none());
    assert!(cfg.event_log_path().is_none());
    assert!(cfg.operator_key_path().is_none());
    assert!(cfg.aggregator_bin_path().is_none());
    assert!(cfg.zk_proving_key_dir().is_none());
    assert!(cfg.eth_rpc_url().is_none());
    assert!(cfg.settlement_private_rpc_url().is_none());
    assert!(!cfg.allow_public_settlement);
    assert!(cfg.contract_address().is_none());
}

#[test]
#[serial]
fn optional_accessors_return_values() {
    clear_env();
    std::env::set_var("DARKPOOL_EVENT_DB", "postgres://host/db");
    std::env::set_var("DARKPOOL_EVENT_LOG", "/tmp/events.log");
    std::env::set_var("DARKPOOL_ETH_RPC", "http://localhost:8545");
    std::env::set_var(
        "DARKPOOL_SETTLEMENT_PRIVATE_RPC",
        "https://rpc.flashbots.net/fast",
    );
    std::env::set_var("DARKPOOL_CONTRACT_ADDR", "0xabc");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(cfg.event_db_url(), Some("postgres://host/db"));
    assert_eq!(cfg.event_log_path(), Some("/tmp/events.log"));
    assert_eq!(cfg.eth_rpc_url(), Some("http://localhost:8545"));
    assert_eq!(
        cfg.settlement_private_rpc_url(),
        Some("https://rpc.flashbots.net/fast")
    );
    assert_eq!(cfg.contract_address(), Some("0xabc"));
    clear_env();
}

#[test]
#[serial]
fn settlement_transport_allows_noop_without_signer() {
    clear_env();
    std::env::set_var("DARKPOOL_ETH_RPC", "http://localhost:8545");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.validate_settlement_transport().is_ok());
    clear_env();
}

#[test]
#[serial]
fn settlement_transport_rejects_signed_public_rpc_by_default() {
    clear_env();
    std::env::set_var("DARKPOOL_ETH_RPC", "http://localhost:8545");
    std::env::set_var("DARKPOOL_SIGNER_KEY_URI", "file:/etc/dp/eth.hex");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    let err = cfg.validate_settlement_transport().unwrap_err();
    assert!(
        err.contains("DARKPOOL_SETTLEMENT_PRIVATE_RPC"),
        "msg: {err}"
    );
    assert!(err.contains("cleared book"), "msg: {err}");
    clear_env();
}

#[test]
#[serial]
fn settlement_transport_allows_private_rpc() {
    clear_env();
    std::env::set_var("DARKPOOL_ETH_RPC", "http://localhost:8545");
    std::env::set_var(
        "DARKPOOL_SETTLEMENT_PRIVATE_RPC",
        "https://rpc.flashbots.net/fast",
    );
    std::env::set_var("DARKPOOL_SIGNER_KEY_URI", "file:/etc/dp/eth.hex");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.validate_settlement_transport().is_ok());
    clear_env();
}

#[test]
#[serial]
fn settlement_transport_allows_explicit_public_dev_override() {
    clear_env();
    std::env::set_var("DARKPOOL_ETH_RPC", "http://localhost:8545");
    std::env::set_var("DARKPOOL_SIGNER_KEY_URI", "file:/etc/dp/eth.hex");
    std::env::set_var("DARKPOOL_ALLOW_PUBLIC_SETTLEMENT", "true");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.allow_public_settlement);
    assert!(cfg.validate_settlement_transport().is_ok());
    clear_env();
}

#[test]
#[serial]
fn settlement_transport_private_rpc_requires_read_rpc() {
    clear_env();
    std::env::set_var(
        "DARKPOOL_SETTLEMENT_PRIVATE_RPC",
        "https://rpc.flashbots.net/fast",
    );
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    let err = cfg.validate_settlement_transport().unwrap_err();
    assert!(err.contains("DARKPOOL_ETH_RPC"), "msg: {err}");
    clear_env();
}

#[test]
#[serial]
fn operator_api_keys_parsed_from_env() {
    clear_env();
    std::env::set_var("DARKPOOL_OPERATOR_API_KEYS", "op1, op2 , op3");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(cfg.operator_api_keys(), vec!["op1", "op2", "op3"]);
    clear_env();
}

#[test]
#[serial]
fn operator_api_keys_empty_by_default() {
    clear_env();
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.operator_api_keys().is_empty());
}

#[test]
#[serial]
fn pair_seed_json_str_is_none_when_unset() {
    clear_env();
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.pair_seed_json_str().is_none());
}

#[test]
#[serial]
fn pair_seed_json_str_returns_value() {
    clear_env();
    let seed = r#"[{"pair":"ETH/USDC","baseToken":"0x0","quoteToken":"0x0"}]"#;
    std::env::set_var("DARKPOOL_PAIR_SEED_JSON", seed);
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(cfg.pair_seed_json_str(), Some(seed));
    clear_env();
}

#[test]
#[serial]
fn operator_key_uris_str_is_none_when_unset() {
    clear_env();
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.operator_key_uris_str().is_none());
}

#[test]
#[serial]
fn operator_key_uris_str_returns_value() {
    clear_env();
    let uris = "file:/etc/dp/active.hex@active,age:/etc/dp/old.age@sunset";
    std::env::set_var("DARKPOOL_OPERATOR_KEY_URIS", uris);
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(cfg.operator_key_uris_str(), Some(uris));
    clear_env();
}

#[test]
#[serial]
fn signer_key_uri_str_is_none_when_unset() {
    clear_env();
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert!(cfg.signer_key_uri_str().is_none());
}

#[test]
#[serial]
fn signer_key_uri_str_returns_value() {
    clear_env();
    let uri = "file:/etc/dp/eth.hex";
    std::env::set_var("DARKPOOL_SIGNER_KEY_URI", uri);
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(cfg.signer_key_uri_str(), Some(uri));
    clear_env();
}

#[test]
#[serial]
fn aggregator_bin_and_zk_proving_key_accessors() {
    clear_env();
    std::env::set_var("DARKPOOL_AGGREGATOR_BIN", "/usr/local/bin/dp-aggregator");
    std::env::set_var("DARKPOOL_OPERATOR_KEY", "/etc/dp/operator.key");
    std::env::set_var("DARKPOOL_ZK_PROVING_KEY", "/etc/dp/keys");
    let cfg = Config::try_parse_from(["bin"]).unwrap();
    assert_eq!(
        cfg.aggregator_bin_path(),
        Some("/usr/local/bin/dp-aggregator")
    );
    assert_eq!(cfg.operator_key_path(), Some("/etc/dp/operator.key"));
    assert_eq!(cfg.zk_proving_key_dir(), Some("/etc/dp/keys"));
    clear_env();
}
