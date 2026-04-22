use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::thread;
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::agent::tools::{ContentPart, ToolResult};

const ACP_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq)]
pub struct AcpAgentMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredAcpAgent {
    pub endpoint: String,
    pub metadata: AcpAgentMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AcpRunStatus {
    Pending,
    Complete(ToolResult),
}

pub trait AcpTransport: Send + Sync + Clone + 'static {
    fn get_agent(&self, endpoint: &str, agent_id: &str)
    -> Result<AcpAgentMetadata, AcpClientError>;
    fn list_agents(&self, endpoint: &str) -> Result<Vec<AcpAgentMetadata>, AcpClientError>;
    fn create_run(
        &self,
        endpoint: &str,
        agent_id: &str,
        input: &Value,
    ) -> Result<String, AcpClientError>;
    fn get_run(&self, endpoint: &str, run_id: &str) -> Result<AcpRunStatus, AcpClientError>;
}

#[derive(Debug, Clone)]
pub struct ReqwestAcpTransport {
    client: Client,
}

impl ReqwestAcpTransport {
    pub fn new() -> Result<Self, AcpClientError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|source| AcpClientError::HttpClient { source })?;
        Ok(Self { client })
    }
}

impl Default for ReqwestAcpTransport {
    fn default() -> Self {
        Self::new().expect("default ACP transport should build")
    }
}

impl AcpTransport for ReqwestAcpTransport {
    fn get_agent(
        &self,
        endpoint: &str,
        agent_id: &str,
    ) -> Result<AcpAgentMetadata, AcpClientError> {
        let url = join_url(endpoint, &format!("agents/{agent_id}"))?;
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|source| AcpClientError::Request {
                endpoint: endpoint.to_string(),
                action: format!("GET /agents/{agent_id}"),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AcpClientError::HttpStatus {
                endpoint: endpoint.to_string(),
                action: format!("GET /agents/{agent_id}"),
                status,
            });
        }
        let payload = response
            .json::<Value>()
            .map_err(|source| AcpClientError::Decode {
                endpoint: endpoint.to_string(),
                action: format!("GET /agents/{agent_id}"),
                source,
            })?;
        parse_agent_metadata(&payload).map_err(|message| AcpClientError::Protocol {
            endpoint: endpoint.to_string(),
            action: format!("GET /agents/{agent_id}"),
            message,
        })
    }

    fn list_agents(&self, endpoint: &str) -> Result<Vec<AcpAgentMetadata>, AcpClientError> {
        let url = join_url(endpoint, "agents")?;
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|source| AcpClientError::Request {
                endpoint: endpoint.to_string(),
                action: "GET /agents".into(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AcpClientError::HttpStatus {
                endpoint: endpoint.to_string(),
                action: "GET /agents".into(),
                status,
            });
        }
        let payload = response
            .json::<Value>()
            .map_err(|source| AcpClientError::Decode {
                endpoint: endpoint.to_string(),
                action: "GET /agents".into(),
                source,
            })?;
        let agents_value = payload.get("agents").cloned().unwrap_or(payload);
        let Some(entries) = agents_value.as_array() else {
            return Err(AcpClientError::Protocol {
                endpoint: endpoint.to_string(),
                action: "GET /agents".into(),
                message: "response must contain an array of agents".into(),
            });
        };
        entries
            .iter()
            .map(|entry| {
                parse_agent_metadata(entry).map_err(|message| AcpClientError::Protocol {
                    endpoint: endpoint.to_string(),
                    action: "GET /agents".into(),
                    message,
                })
            })
            .collect()
    }

    fn create_run(
        &self,
        endpoint: &str,
        agent_id: &str,
        input: &Value,
    ) -> Result<String, AcpClientError> {
        let url = join_url(endpoint, "runs")?;
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "agent_id": agent_id,
                "input": input,
            }))
            .send()
            .map_err(|source| AcpClientError::Request {
                endpoint: endpoint.to_string(),
                action: "POST /runs".into(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AcpClientError::HttpStatus {
                endpoint: endpoint.to_string(),
                action: "POST /runs".into(),
                status,
            });
        }
        let payload = response
            .json::<Value>()
            .map_err(|source| AcpClientError::Decode {
                endpoint: endpoint.to_string(),
                action: "POST /runs".into(),
                source,
            })?;
        payload
            .get("id")
            .or_else(|| payload.get("run_id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| AcpClientError::Protocol {
                endpoint: endpoint.to_string(),
                action: "POST /runs".into(),
                message: "response must contain run id".into(),
            })
    }

    fn get_run(&self, endpoint: &str, run_id: &str) -> Result<AcpRunStatus, AcpClientError> {
        let url = join_url(endpoint, &format!("runs/{run_id}"))?;
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|source| AcpClientError::Request {
                endpoint: endpoint.to_string(),
                action: format!("GET /runs/{run_id}"),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AcpClientError::HttpStatus {
                endpoint: endpoint.to_string(),
                action: format!("GET /runs/{run_id}"),
                status,
            });
        }
        let payload = response
            .json::<Value>()
            .map_err(|source| AcpClientError::Decode {
                endpoint: endpoint.to_string(),
                action: format!("GET /runs/{run_id}"),
                source,
            })?;
        parse_run_status(&payload).map_err(|message| AcpClientError::Protocol {
            endpoint: endpoint.to_string(),
            action: format!("GET /runs/{run_id}"),
            message,
        })
    }
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

#[derive(Debug)]
pub enum AcpClientError {
    InvalidEndpoint {
        endpoint: String,
        source: String,
    },
    HttpClient {
        source: reqwest::Error,
    },
    Request {
        endpoint: String,
        action: String,
        source: reqwest::Error,
    },
    HttpStatus {
        endpoint: String,
        action: String,
        status: reqwest::StatusCode,
    },
    Decode {
        endpoint: String,
        action: String,
        source: reqwest::Error,
    },
    Protocol {
        endpoint: String,
        action: String,
        message: String,
    },
}

impl Display for AcpClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEndpoint { endpoint, source } => {
                write!(f, "invalid ACP endpoint {endpoint}: {source}")
            }
            Self::HttpClient { source } => write!(f, "cannot build ACP HTTP client: {source}"),
            Self::Request {
                endpoint,
                action,
                source,
            } => write!(f, "{action} against {endpoint} failed: {source}"),
            Self::HttpStatus {
                endpoint,
                action,
                status,
            } => write!(f, "{action} against {endpoint} returned HTTP {status}"),
            Self::Decode {
                endpoint,
                action,
                source,
            } => write!(
                f,
                "{action} against {endpoint} returned invalid JSON: {source}"
            ),
            Self::Protocol {
                endpoint,
                action,
                message,
            } => write!(
                f,
                "{action} against {endpoint} returned invalid ACP payload: {message}"
            ),
        }
    }
}

impl std::error::Error for AcpClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::HttpClient { source } => Some(source),
            Self::Request { source, .. } => Some(source),
            Self::Decode { source, .. } => Some(source),
            Self::InvalidEndpoint { .. } | Self::HttpStatus { .. } | Self::Protocol { .. } => None,
        }
    }
}

pub fn normalize_endpoint(endpoint: &str) -> String {
    endpoint.trim_end_matches('/').to_string()
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

fn join_url(endpoint: &str, path: &str) -> Result<Url, AcpClientError> {
    let normalized = format!("{}/", normalize_endpoint(endpoint));
    let base = Url::parse(&normalized).map_err(|source| AcpClientError::InvalidEndpoint {
        endpoint: endpoint.to_string(),
        source: source.to_string(),
    })?;
    base.join(path)
        .map_err(|source| AcpClientError::InvalidEndpoint {
            endpoint: endpoint.to_string(),
            source: source.to_string(),
        })
}

fn parse_agent_metadata(value: &Value) -> Result<AcpAgentMetadata, String> {
    let wire: AgentMetadataWire =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if wire.id.trim().is_empty() {
        return Err("agent id must not be empty".into());
    }
    let name = wire
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| wire.id.clone());
    Ok(AcpAgentMetadata {
        id: wire.id,
        name,
        description: wire.description.unwrap_or_default(),
        input_schema: wire
            .input_schema
            .unwrap_or_else(|| serde_json::json!({"type": "object"})),
    })
}

fn parse_run_status(value: &Value) -> Result<AcpRunStatus, String> {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "run status must be a string".to_string())?;

    match status {
        "queued" | "running" | "pending" | "in_progress" => Ok(AcpRunStatus::Pending),
        "completed" | "succeeded" => {
            let output_value = value
                .get("output")
                .or_else(|| value.get("result"))
                .ok_or_else(|| format!("run with status '{status}' must contain output"))?;
            parse_tool_result(output_value).map(AcpRunStatus::Complete)
        }
        "failed" | "cancelled" | "canceled" => {
            if let Some(output_value) = value.get("output").or_else(|| value.get("result")) {
                parse_tool_result(output_value).map(AcpRunStatus::Complete)
            } else {
                Ok(AcpRunStatus::Complete(ToolResult::failure(format!(
                    "ACP run ended with status '{status}'"
                ))))
            }
        }
        other => Err(format!("unknown ACP run status '{other}'")),
    }
}

fn parse_tool_result(value: &Value) -> Result<ToolResult, String> {
    let is_error = value
        .get("is_error")
        .or_else(|| value.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let output_truncated = value
        .get("output_truncated")
        .or_else(|| value.get("outputTruncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let exit_code = value
        .get("exit_code")
        .or_else(|| value.get("exitCode"))
        .and_then(Value::as_i64)
        .map(|code| i32::try_from(code).map_err(|_| "exit code must fit within i32".to_string()))
        .transpose()?;

    let content = if let Some(content) = value.get("content") {
        parse_content_parts(content)?
    } else if let Some(text) = value.get("text").and_then(Value::as_str) {
        vec![ContentPart::text(text)]
    } else {
        return Err("tool output must contain content or text".into());
    };

    Ok(ToolResult {
        content,
        exit_code,
        is_error,
        output_truncated,
    })
}

fn parse_content_parts(value: &Value) -> Result<Vec<ContentPart>, String> {
    let Some(entries) = value.as_array() else {
        return Err("content must be an array".into());
    };

    let mut parts = Vec::with_capacity(entries.len());
    for entry in entries {
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("text");
        match kind {
            "text" => {
                let text = entry
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "text content entry must contain text".to_string())?;
                parts.push(ContentPart::text(text));
            }
            "resource" => {
                let uri = entry
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "resource content entry must contain uri".to_string())?;
                let mime_type = entry
                    .get("mime_type")
                    .or_else(|| entry.get("mimeType"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                parts.push(ContentPart::Resource {
                    uri: uri.to_string(),
                    mime_type,
                });
            }
            other => return Err(format!("unsupported ACP content part type: {other}")),
        }
    }

    Ok(parts)
}

#[derive(Debug, Deserialize)]
struct AgentMetadataWire {
    id: String,
    name: Option<String>,
    description: Option<String>,
    #[serde(default, alias = "inputSchema")]
    input_schema: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_agent_metadata_falls_back_to_id_for_blank_name() {
        let metadata = parse_agent_metadata(&json!({
            "id": "reviewer",
            "name": "   ",
            "description": "Reviews code"
        }))
        .expect("metadata should parse");

        assert_eq!(metadata.name, "reviewer");
    }

    #[test]
    fn parse_run_status_rejects_exit_code_overflow() {
        let error = parse_run_status(&json!({
            "status": "completed",
            "output": {
                "content": [{ "type": "text", "text": "done" }],
                "exit_code": i64::from(i32::MAX) + 1
            }
        }))
        .expect_err("overflowing exit code should fail");

        assert!(error.contains("exit code must fit within i32"));
    }

    #[test]
    fn parse_run_status_rejects_unknown_status_values() {
        let error = parse_run_status(&json!({
            "status": "mystery-state"
        }))
        .expect_err("unknown statuses should be rejected explicitly");

        assert!(error.contains("mystery-state"));
    }
}
