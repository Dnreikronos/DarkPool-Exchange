//! Exercises `init_tracing` once in a dedicated test binary so the
//! process-global subscriber install path is covered. A second call
//! within the same binary deterministically hits the
//! "registry already initialised" error, so we cover both branches in
//! a single test function (test ordering across files is not
//! guaranteed, but ordering *within* a test is).

use dp_api::observability::{init_tracing, ObservabilityError};

#[test]
fn init_tracing_installs_subscriber_then_rejects_second_call() {
    // Force the text-formatter branch with no OTLP endpoint so the test
    // does not try to reach out over the network.
    std::env::set_var("DP_LOG_FORMAT", "text");
    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");

    let guard = init_tracing().expect("first init_tracing must succeed");

    // Drop the guard on the happy path: when no OTLP provider was
    // installed this is a no-op flush, exercising the `provider: None`
    // branch in `Drop`.
    drop(guard);

    // The tracing-subscriber registry is process-global; a second
    // init must surface the wrapped error, not panic.
    match init_tracing() {
        Ok(_) => panic!("second init_tracing must fail"),
        Err(ObservabilityError::Setup(msg)) => {
            assert!(
                !msg.is_empty(),
                "Setup error message must not be empty"
            );
        }
    }
}
