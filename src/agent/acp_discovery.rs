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
