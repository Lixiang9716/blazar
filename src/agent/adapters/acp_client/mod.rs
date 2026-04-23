mod client;
mod mapper;

use serde_json::json;

use super::conformance::AgentAdapterContractProbe;
use mapper::{parse_agent_metadata, parse_run_status};

pub use client::{AcpClientError, AcpTransport, ReqwestAcpTransport, normalize_endpoint};
pub use mapper::{AcpAgentMetadata, AcpRunStatus};

#[derive(Debug, Default, Clone, Copy)]
pub struct AcpAdapterContractProbe {
    _private: (),
}

impl AgentAdapterContractProbe for AcpAdapterContractProbe {
    fn fetch_agent(&self) -> Result<(), String> {
        parse_agent_metadata(&json!({
            "id": "reviewer",
            "name": "ACP Reviewer",
            "description": "Reviews code",
            "input_schema": { "type": "object" }
        }))
        .map(|_| ())
    }

    fn create_run(&self) -> Result<(), String> {
        let payload = json!({ "id": "run-123" });
        payload
            .get("id")
            .or_else(|| payload.get("run_id"))
            .and_then(serde_json::Value::as_str)
            .map(|_| ())
            .ok_or_else(|| "response must contain run id".to_string())
    }

    fn poll_terminal(&self) -> Result<(), String> {
        match parse_run_status(&json!({
            "status": "completed",
            "output": { "text": "done" }
        }))? {
            AcpRunStatus::Pending => Err("completed status must map to terminal result".into()),
            AcpRunStatus::Complete(_) => Ok(()),
        }
    }
}
