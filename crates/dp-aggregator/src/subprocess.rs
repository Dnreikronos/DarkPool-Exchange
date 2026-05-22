use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use dp_auction::Match;
use dp_zk::witness::BatchWitness;
use serde::Serialize;
use tracing::Instrument;
use uuid::Uuid;

use crate::aggregator::ProofAggregator;
use crate::AggregatorError;

#[derive(Debug)]
pub struct SubprocessAggregator {
    bin_path: PathBuf,
    timeout: Duration,
    env: Vec<(String, String)>,
}

#[derive(Serialize)]
struct AggregatorInput<'a> {
    batch_id: String,
    matches: Vec<AggregatorMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_witness: Option<&'a [dp_zk::witness::MatchWitness]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<&'a dp_zk::witness::Policy>,
}

#[derive(Serialize)]
struct AggregatorMatch {
    auction_id: String,
    bid_order_id: String,
    ask_order_id: String,
    price: String,
    size: String,
}

impl SubprocessAggregator {
    pub fn new(bin_path: &Path, timeout: Option<Duration>) -> Result<Self, AggregatorError> {
        if !bin_path.exists() {
            return Err(AggregatorError::BinaryNotFound(bin_path.to_path_buf()));
        }
        Ok(Self {
            bin_path: bin_path.to_path_buf(),
            timeout: timeout.unwrap_or(Duration::from_secs(30)),
            env: Vec::new(),
        })
    }

    /// Per-spawn env var passed to the child via `Command::env`. Use this
    /// instead of `std::env::set_var` — the latter is undefined behaviour
    /// in a multi-threaded process (Rust ≥ 1.74).
    pub fn with_env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

/// Build the tracing span for a single `aggregate` call. Extracted so
/// unit tests can verify the span's fields without spawning a real
/// subprocess.
fn build_aggregate_span(batch_id: Uuid, auction_id: Uuid, match_count: usize) -> tracing::Span {
    tracing::info_span!(
        "dp_aggregator.subprocess.aggregate",
        batch_id = %batch_id,
        auction_id = %auction_id,
        match_count = match_count,
    )
}

impl ProofAggregator for SubprocessAggregator {
    fn aggregate<'a>(
        &'a self,
        batch_id: Uuid,
        auction_id: Uuid,
        matches: &'a [Match],
        witness: &'a BatchWitness,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
        let span = build_aggregate_span(batch_id, auction_id, matches.len());
        Box::pin(
            async move {
                let private_witness = if witness.matches.is_empty() {
                    None
                } else {
                    Some(witness.matches.as_slice())
                };
                let input = AggregatorInput {
                    batch_id: batch_id.to_string(),
                    matches: matches
                        .iter()
                        .map(|m| AggregatorMatch {
                            auction_id: auction_id.to_string(),
                            bid_order_id: m.bid.order_id.to_string(),
                            ask_order_id: m.ask.order_id.to_string(),
                            price: m.price.to_string(),
                            size: m.size.to_string(),
                        })
                        .collect(),
                    private_witness,
                    policy: Some(&witness.policy),
                };
                let payload = serde_json::to_vec(&input)?;

                let mut cmd = tokio::process::Command::new(&self.bin_path);
                for (k, v) in &self.env {
                    cmd.env(k, v);
                }
                let mut child = cmd
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()?;

                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                let mut stdin = child.stdin.take().unwrap();
                stdin.write_all(&payload).await?;
                stdin.shutdown().await?;
                // shutdown() does not close the fd — drop the handle so the
                // child observes EOF and proceeds.
                drop(stdin);

                let mut stdout_handle = child.stdout.take().unwrap();
                let mut stderr_handle = child.stderr.take().unwrap();

                let result = tokio::time::timeout(self.timeout, async {
                    let (stdout_res, stderr_res) = tokio::join!(
                        async {
                            let mut buf = Vec::new();
                            stdout_handle.read_to_end(&mut buf).await.map(|_| buf)
                        },
                        async {
                            let mut buf = Vec::new();
                            stderr_handle.read_to_end(&mut buf).await.map(|_| buf)
                        }
                    );
                    let stdout = stdout_res?;
                    let stderr = stderr_res?;
                    Ok::<_, std::io::Error>((stdout, stderr))
                })
                .await;

                match result {
                    Ok(Ok((stdout, stderr))) => {
                        let status = child.wait().await?;
                        if !status.success() {
                            return Err(AggregatorError::ProcessFailed {
                                exit_code: status.code().unwrap_or(-1),
                                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                            });
                        }
                        let mut proof = stdout;
                        while proof.last() == Some(&b'\n') {
                            proof.pop();
                        }
                        Ok(proof)
                    }
                    Ok(Err(e)) => Err(AggregatorError::Io(e)),
                    Err(_) => {
                        let _ = child.kill().await;
                        Err(AggregatorError::Timeout)
                    }
                }
            }
            .instrument(span),
        )
    }

    // The subprocess aggregator drives a Groth16 prover binary and does not
    // support IVC fold/finalize. The tick loop migrated to the IVC path — any
    // deployment still using SubprocessAggregator will get a loud error on
    // every tick rather than silently calling the dead `aggregate` path.
    fn fold_step<'a>(
        &'a self,
        _pair: String,
        _external_inputs: dp_zk::step_circuit::AuctionExternalInputs,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
            Err(AggregatorError::Zk(
                "SubprocessAggregator does not support IVC fold_step; \
                 switch to InlineIvcAggregator for the HyperNova path".into(),
            ))
        })
    }

    fn finalize<'a>(
        &'a self,
        _pair: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<dp_zk::folding::FinalProof, AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
            Err(AggregatorError::Zk(
                "SubprocessAggregator does not support IVC finalize; \
                 switch to InlineIvcAggregator for the HyperNova path".into(),
            ))
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use dp_auction::Match;
    use dp_types::Fill;
    use rust_decimal::Decimal;
    use tempfile::TempDir;

    fn make_executable(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("script.sh");
        std::fs::write(&path, content).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
    }

    fn test_matches() -> Vec<Match> {
        vec![Match {
            bid: Fill {
                order_id: Uuid::new_v4(),
                size: Decimal::from(10),
            },
            ask: Fill {
                order_id: Uuid::new_v4(),
                size: Decimal::from(10),
            },
            price: Decimal::from(100),
            size: Decimal::from(10),
        }]
    }

    fn empty_witness() -> BatchWitness {
        BatchWitness::empty(Uuid::nil(), Uuid::nil())
    }

    #[tokio::test]
    async fn happy_path() {
        let (_dir, path) = make_executable("#!/bin/sh\nprintf 'deadbeef'");
        let agg = SubprocessAggregator::new(&path, None).unwrap();
        let proof = agg
            .aggregate(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &test_matches(),
                &empty_witness(),
            )
            .await
            .unwrap();
        assert_eq!(proof, b"deadbeef");
    }

    #[tokio::test]
    async fn non_zero_exit() {
        let (_dir, path) = make_executable("#!/bin/sh\necho 'boom' >&2\nexit 3");
        let agg = SubprocessAggregator::new(&path, None).unwrap();
        let err = agg
            .aggregate(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &test_matches(),
                &empty_witness(),
            )
            .await
            .unwrap_err();
        match err {
            AggregatorError::ProcessFailed { exit_code, stderr } => {
                assert_eq!(exit_code, 3);
                assert!(stderr.contains("boom"));
            }
            other => panic!("expected ProcessFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn timeout() {
        let (_dir, path) = make_executable("#!/bin/sh\nsleep 60");
        let agg = SubprocessAggregator::new(&path, Some(Duration::from_millis(100))).unwrap();
        let err = agg
            .aggregate(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &test_matches(),
                &empty_witness(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AggregatorError::Timeout));
    }

    #[tokio::test]
    async fn missing_binary() {
        let err = SubprocessAggregator::new(Path::new("/no/such/binary"), None).unwrap_err();
        assert!(matches!(err, AggregatorError::BinaryNotFound(_)));
    }

    #[test]
    fn build_aggregate_span_records_batch_metadata() {
        // Install a thread-local subscriber so span metadata is alive
        // for the assertion. Without a subscriber `Span::metadata()`
        // returns `None` and we cannot inspect the field set.
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(tracing_subscriber::fmt::layer());
        let _guard = tracing::subscriber::set_default(subscriber);

        let span = build_aggregate_span(Uuid::new_v4(), Uuid::new_v4(), 4);
        let metadata = span.metadata().expect("subscriber attached, span enabled");
        assert_eq!(metadata.name(), "dp_aggregator.subprocess.aggregate");
        let fields: Vec<&str> = metadata.fields().iter().map(|f| f.name()).collect();
        assert!(fields.contains(&"batch_id"), "fields: {fields:?}");
        assert!(fields.contains(&"auction_id"), "fields: {fields:?}");
        assert!(fields.contains(&"match_count"), "fields: {fields:?}");
    }
}
