use std::fmt::{self, Display, Formatter};
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::Client;
use serde_json::Value;

use super::mapper::{AcpAgentMetadata, AcpRunStatus, parse_agent_metadata, parse_run_status};

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

#[cfg(test)]
#[path = "../../../../tests/unit/agent/adapters/acp_client/client_tests.rs"]
mod tests;
