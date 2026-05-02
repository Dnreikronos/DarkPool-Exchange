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
        "DARKPOOL_RATE_LIMIT",
        "DARKPOOL_RATE_BURST",
        "DARKPOOL_RATE_STALE_AFTER",
        "DARKPOOL_EVENT_LOG",
        "DARKPOOL_OPERATOR_KEY",
        "DARKPOOL_AGGREGATOR_BIN",
        "DARKPOOL_AGGREGATOR_TIMEOUT",
        "DARKPOOL_SUBMIT_TIMEOUT",
        "DARKPOOL_ETH_RPC",
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
