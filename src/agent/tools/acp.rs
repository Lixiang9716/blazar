use serde_json::Value;

use super::{AgentProtocol, Tool, ToolKind, ToolResult, ToolSpec};
use crate::agent::acp_discovery::{
    AcpAgentMetadata, AcpClientError, AcpTransport, ReqwestAcpTransport, poll_run_to_completion,
};

const MAX_RUN_POLLS: usize = 40;

// The official Agent Client Protocol Rust SDK is aimed primarily at stdio/tokio
// role wiring. Blazar currently needs a blocking tool-facing adapter for ACP
// HTTP endpoints, so we keep that transport behind `AcpTransport` to make a
// later SDK-backed bridge a localized follow-up instead of a runtime rewrite.
pub struct AcpAgentTool<T = ReqwestAcpTransport> {
    tool_name: String,
    endpoint: String,
    metadata: AcpAgentMetadata,
    transport: T,
}

impl AcpAgentTool<ReqwestAcpTransport> {
    pub fn new(
        tool_name: impl Into<String>,
        endpoint: impl Into<String>,
        metadata: AcpAgentMetadata,
    ) -> Result<Self, AcpClientError> {
        Ok(Self::with_transport(
            tool_name,
            endpoint,
            metadata,
            ReqwestAcpTransport::new()?,
        ))
    }
}

impl<T> AcpAgentTool<T> {
    pub fn with_transport(
        tool_name: impl Into<String>,
        endpoint: impl Into<String>,
        metadata: AcpAgentMetadata,
        transport: T,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            endpoint: endpoint.into(),
            metadata,
            transport,
        }
    }
}

impl<T> Tool for AcpAgentTool<T>
where
    T: AcpTransport,
{
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.tool_name.clone(),
            description: self.metadata.description.clone(),
            parameters: self.metadata.input_schema.clone(),
        }
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Agent {
            protocol: AgentProtocol::Acp,
        }
    }

    fn execute(&self, args: Value) -> ToolResult {
        let run_id = match self
            .transport
            .create_run(&self.endpoint, &self.metadata.id, &args)
        {
            Ok(run_id) => run_id,
            Err(error) => {
                return ToolResult::failure(format!("agent unreachable: {}", error));
            }
        };

        match poll_run_to_completion(&self.transport, &self.endpoint, &run_id, MAX_RUN_POLLS) {
            Ok(result) => result,
            Err(error) => ToolResult::failure(format!("ACP run failed: {error}")),
        }
    }
}

#[cfg(test)]
#[path = "acp/tests.rs"]
mod tests;
