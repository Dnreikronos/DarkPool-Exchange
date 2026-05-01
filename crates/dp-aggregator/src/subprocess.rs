use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use dp_auction::Match;
use serde::Serialize;
use uuid::Uuid;

use crate::aggregator::ProofAggregator;
use crate::AggregatorError;

#[derive(Debug)]
pub struct SubprocessAggregator {
    bin_path: PathBuf,
    timeout: Duration,
}

#[derive(Serialize)]
struct AggregatorInput {
    batch_id: String,
    matches: Vec<AggregatorMatch>,
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
        })
    }
}

impl ProofAggregator for SubprocessAggregator {
    fn aggregate<'a>(
        &'a self,
        batch_id: Uuid,
        auction_id: Uuid,
        matches: &'a [Match],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AggregatorError>> + Send + 'a>> {
        Box::pin(async move {
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
            };
            let payload = serde_json::to_vec(&input)?;

            let mut child = tokio::process::Command::new(&self.bin_path)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(&payload).await?;
            stdin.shutdown().await?;

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

    #[tokio::test]
    async fn happy_path() {
        let (_dir, path) = make_executable("#!/bin/sh\nprintf 'deadbeef'");
        let agg = SubprocessAggregator::new(&path, None).unwrap();
        let proof = agg
            .aggregate(Uuid::new_v4(), Uuid::new_v4(), &test_matches())
            .await
            .unwrap();
        assert_eq!(proof, b"deadbeef");
    }

    #[tokio::test]
    async fn non_zero_exit() {
        let (_dir, path) = make_executable("#!/bin/sh\necho 'boom' >&2\nexit 3");
        let agg = SubprocessAggregator::new(&path, None).unwrap();
        let err = agg
            .aggregate(Uuid::new_v4(), Uuid::new_v4(), &test_matches())
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
        let agg =
            SubprocessAggregator::new(&path, Some(Duration::from_millis(100))).unwrap();
        let err = agg
            .aggregate(Uuid::new_v4(), Uuid::new_v4(), &test_matches())
            .await
            .unwrap_err();
        assert!(matches!(err, AggregatorError::Timeout));
    }

    #[tokio::test]
    async fn missing_binary() {
        let err = SubprocessAggregator::new(Path::new("/no/such/binary"), None).unwrap_err();
        assert!(matches!(err, AggregatorError::BinaryNotFound(_)));
    }
}
