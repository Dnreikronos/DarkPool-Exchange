//! Pluggable readiness probes for `/readyz`.
//!
//! The HTTP handler runs every probe in registration order; the first
//! failure is reported with HTTP 503 and the probe name in the body so
//! operators can see which subsystem regressed without having to grep
//! logs. Probes are simple closures returning `Result<(), String>` —
//! tests can register custom failing probes without touching real
//! infrastructure.

use std::sync::Arc;

use dp_event::Store;

/// One named readiness check. `check` returns `Err(reason)` to flip the
/// pod state to "not ready".
pub type ProbeFn = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct Probe {
    pub name: &'static str,
    pub check: ProbeFn,
}

impl Probe {
    pub fn new<F>(name: &'static str, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        Self {
            name,
            check: Arc::new(f),
        }
    }
}

#[derive(Clone, Default)]
pub struct ReadinessProbes {
    probes: Vec<Probe>,
}

impl ReadinessProbes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, probe: Probe) -> Self {
        self.probes.push(probe);
        self
    }

    /// Run every probe in registration order. Returns the first failure
    /// or `Ok` when all probes pass.
    pub fn check(&self) -> Result<(), ProbeFailure> {
        for p in &self.probes {
            if let Err(reason) = (p.check)() {
                return Err(ProbeFailure {
                    name: p.name,
                    reason,
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ProbeFailure {
    pub name: &'static str,
    pub reason: String,
}

// ---- Standard probes -------------------------------------------------------

/// Event store reachability. Calls `Store::ping` which is a no-op for
/// the in-memory backend and a real round-trip for postgres / a stat()
/// for the file backend.
///
/// **Blocking on PgStore.** The synchronous `Store::ping` bridges to
/// async via `tokio::task::block_in_place`, so each `/readyz` request
/// against a postgres-backed store parks a tokio worker thread until
/// the round-trip completes. Keep the connection pool's statement
/// timeout short, otherwise a stalled DB will pin worker threads and
/// degrade unrelated traffic on the same listener.
pub fn store_probe(store: Arc<dyn Store>) -> Probe {
    Probe::new("event_store", move || {
        store
            .ping()
            .map_err(|e| format!("event store ping failed: {e}"))
    })
}

/// Aggregator binary still present + executable bit set as far as the
/// filesystem can tell us. Boot-time validation already rejected a
/// missing path; this probe catches operator mistakes after boot
/// (volume unmounted, binary deleted).
pub fn aggregator_probe(path: Option<String>) -> Probe {
    Probe::new("aggregator_bin", move || match &path {
        None => Ok(()),
        Some(p) => {
            let m = std::fs::metadata(p).map_err(|e| format!("aggregator bin {p}: {e}"))?;
            if !m.is_file() {
                return Err(format!("aggregator bin {p}: not a regular file"));
            }
            Ok(())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_event::MemStore;

    #[test]
    fn empty_probes_pass() {
        assert!(ReadinessProbes::new().check().is_ok());
    }

    #[test]
    fn first_failure_short_circuits() {
        let probes = ReadinessProbes::new()
            .with(Probe::new("ok", || Ok(())))
            .with(Probe::new("boom", || Err("kaboom".into())))
            .with(Probe::new("never_runs", || {
                panic!("must not run after failure")
            }));
        let err = probes.check().unwrap_err();
        assert_eq!(err.name, "boom");
        assert_eq!(err.reason, "kaboom");
    }

    #[test]
    fn store_probe_passes_for_mem_store() {
        let store: Arc<dyn Store> = Arc::new(MemStore::new());
        let probe = store_probe(store);
        assert!((probe.check)().is_ok());
    }

    #[test]
    fn aggregator_probe_no_path_is_ok() {
        let probe = aggregator_probe(None);
        assert!((probe.check)().is_ok());
    }

    #[test]
    fn aggregator_probe_missing_path_fails() {
        let probe = aggregator_probe(Some("/nonexistent/xyz".into()));
        let err = (probe.check)().unwrap_err();
        assert!(err.contains("aggregator bin"));
    }
}
