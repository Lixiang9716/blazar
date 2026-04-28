use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use crate::agent::tools::ToolResult;

pub use crate::agent::adapters::acp_client::{
    AcpAgentMetadata, AcpClientError, AcpRunStatus, AcpTransport, ReqwestAcpTransport,
    normalize_endpoint,
};

const ACP_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredAcpAgent {
    pub endpoint: String,
    pub metadata: AcpAgentMetadata,
}

#[derive(Debug, Default)]
pub struct AcpDiscoveryReport {
    pub agents: Vec<DiscoveredAcpAgent>,
    pub errors: Vec<AcpClientError>,
}

#[derive(Debug, Clone)]
pub struct AcpDiscovery<T = ReqwestAcpTransport> {
    endpoints: Vec<String>,
    transport: T,
}

impl<T> AcpDiscovery<T> {
    pub fn new(endpoints: Vec<String>, transport: T) -> Self {
        Self {
            endpoints,
            transport,
        }
    }
}

impl<T> AcpDiscovery<T>
where
    T: AcpTransport,
{
    pub fn discover(&self, existing: &HashSet<(String, String)>) -> AcpDiscoveryReport {
        let mut report = AcpDiscoveryReport::default();
        let mut seen = existing.clone();

        for endpoint in &self.endpoints {
            match self.transport.list_agents(endpoint) {
                Ok(agents) => {
                    for metadata in agents {
                        let key = (normalize_endpoint(endpoint), metadata.id.clone());
                        if seen.insert(key) {
                            report.agents.push(DiscoveredAcpAgent {
                                endpoint: normalize_endpoint(endpoint),
                                metadata,
                            });
                        }
                    }
                }
                Err(error) => report.errors.push(error),
            }
        }

        report
    }
}

pub fn poll_run_to_completion<T: AcpTransport>(
    transport: &T,
    endpoint: &str,
    run_id: &str,
    max_polls: usize,
) -> Result<ToolResult, AcpClientError> {
    for _ in 0..max_polls {
        match transport.get_run(endpoint, run_id)? {
            AcpRunStatus::Pending => thread::sleep(ACP_POLL_INTERVAL),
            AcpRunStatus::Complete(result) => return Ok(result),
        }
    }

    Ok(ToolResult::failure(format!(
        "ACP run {run_id} timed out after {max_polls} polls"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::adapters::acp_client::{AcpAgentMetadata, AcpClientError, AcpRunStatus};
    use crate::agent::tools::ToolResult;
    use serde_json::Value;

    #[derive(Clone)]
    struct FakeTransport;

    impl AcpTransport for FakeTransport {
        fn get_agent(
            &self,
            _endpoint: &str,
            _agent_id: &str,
        ) -> Result<AcpAgentMetadata, AcpClientError> {
            unimplemented!()
        }

        fn list_agents(&self, _endpoint: &str) -> Result<Vec<AcpAgentMetadata>, AcpClientError> {
            Ok(vec![])
        }

        fn create_run(
            &self,
            _endpoint: &str,
            _agent_id: &str,
            _input: &Value,
        ) -> Result<String, AcpClientError> {
            unimplemented!()
        }

        fn get_run(&self, _endpoint: &str, _run_id: &str) -> Result<AcpRunStatus, AcpClientError> {
            // Always returns Pending so the poll loop exhausts max_polls.
            Ok(AcpRunStatus::Pending)
        }
    }

    #[test]
    fn poll_run_to_completion_returns_timeout_after_max_polls() {
        let transport = FakeTransport;
        let result = poll_run_to_completion(&transport, "http://example.com", "run-1", 3)
            .expect("should not return Err");

        assert!(result.is_error);
        assert!(
            result.text_output().contains("timed out"),
            "expected timeout message, got: {}",
            result.text_output()
        );
    }

    #[test]
    fn poll_run_to_completion_returns_result_after_pending_polls() {
        // Transport that returns Pending twice then Complete.
        #[derive(Clone)]
        struct PendThenComplete {
            calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }

        impl AcpTransport for PendThenComplete {
            fn get_agent(&self, _: &str, _: &str) -> Result<AcpAgentMetadata, AcpClientError> {
                unimplemented!()
            }
            fn list_agents(&self, _: &str) -> Result<Vec<AcpAgentMetadata>, AcpClientError> {
                Ok(vec![])
            }
            fn create_run(&self, _: &str, _: &str, _: &Value) -> Result<String, AcpClientError> {
                unimplemented!()
            }
            fn get_run(&self, _: &str, _: &str) -> Result<AcpRunStatus, AcpClientError> {
                let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 2 {
                    Ok(AcpRunStatus::Pending)
                } else {
                    Ok(AcpRunStatus::Complete(ToolResult::success("done")))
                }
            }
        }

        let transport = PendThenComplete {
            calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };
        let result = poll_run_to_completion(&transport, "http://example.com", "run-2", 10)
            .expect("should succeed");

        assert!(!result.is_error);
        assert_eq!(result.text_output(), "done");
    }
}
