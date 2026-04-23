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
mod tests {
    use httpmock::prelude::*;
    use serde_json::json;

    use super::*;

    #[test]
    fn get_agent_parses_successful_payload() {
        let server = MockServer::start();
        let get_agent = server.mock(|when, then| {
            when.method(GET).path("/agents/reviewer");
            then.status(200).json_body(json!({
                "id": "reviewer",
                "name": "ACP Reviewer",
                "description": "Reviews code changes",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "prompt": { "type": "string" }
                    }
                }
            }));
        });

        let transport = ReqwestAcpTransport::new().expect("transport should initialize");
        let metadata = transport
            .get_agent(&server.base_url(), "reviewer")
            .expect("agent metadata should parse");

        assert_eq!(metadata.id, "reviewer");
        assert_eq!(metadata.name, "ACP Reviewer");
        assert_eq!(metadata.description, "Reviews code changes");
        get_agent.assert();
    }

    #[test]
    fn get_agent_maps_http_status_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/agents/missing");
            then.status(404);
        });

        let transport = ReqwestAcpTransport::new().expect("transport should initialize");
        let error = transport
            .get_agent(&server.base_url(), "missing")
            .expect_err("missing agent should return an error");

        match error {
            AcpClientError::HttpStatus {
                endpoint,
                action,
                status,
            } => {
                assert_eq!(endpoint, server.base_url());
                assert_eq!(action, "GET /agents/missing");
                assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
            }
            other => panic!("expected HTTP status error, got {other:?}"),
        }
    }

    #[test]
    fn list_agents_parses_successful_payload() {
        let server = MockServer::start();
        let list_agents = server.mock(|when, then| {
            when.method(GET).path("/agents");
            then.status(200).json_body(json!({
                "agents": [
                    {
                        "id": "reviewer",
                        "name": "ACP Reviewer",
                        "description": "Reviews code",
                        "input_schema": { "type": "object" }
                    },
                    {
                        "id": "planner",
                        "name": "ACP Planner",
                        "description": "Plans implementation",
                        "input_schema": { "type": "object" }
                    }
                ]
            }));
        });

        let transport = ReqwestAcpTransport::new().expect("transport should initialize");
        let agents = transport
            .list_agents(&server.base_url())
            .expect("agent list should parse");

        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].id, "reviewer");
        assert_eq!(agents[1].id, "planner");
        list_agents.assert();
    }

    #[test]
    fn list_agents_maps_protocol_errors_for_non_array_payload() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/agents");
            then.status(200).json_body(json!({
                "agents": { "id": "not-an-array" }
            }));
        });

        let transport = ReqwestAcpTransport::new().expect("transport should initialize");
        let error = transport
            .list_agents(&server.base_url())
            .expect_err("non-array payload should fail");

        match error {
            AcpClientError::Protocol {
                endpoint,
                action,
                message,
            } => {
                assert_eq!(endpoint, server.base_url());
                assert_eq!(action, "GET /agents");
                assert!(
                    message.contains("array of agents"),
                    "unexpected protocol message: {message}"
                );
            }
            other => panic!("expected protocol error, got {other:?}"),
        }
    }
}
