use std::sync::mpsc::Sender;

use crate::agent::tools::ToolSpec;

use super::{LlmProvider, ProviderEvent, ProviderMessage};

/// A provider that echoes the user prompt back, one character at a time.
///
/// Useful for testing the full agent pipeline without external services.
pub struct EchoProvider {
    delay_ms: u64,
}

impl EchoProvider {
    pub fn new(delay_ms: u64) -> Self {
        Self { delay_ms }
    }
}

impl Default for EchoProvider {
    fn default() -> Self {
        Self { delay_ms: 30 }
    }
}

impl LlmProvider for EchoProvider {
    fn stream_turn(
        &self,
        _model: &str,
        messages: &[ProviderMessage],
        _tools: &[ToolSpec],
        tx: Sender<ProviderEvent>,
    ) {
        let prompt = messages
            .iter()
            .rev()
            .find_map(|message| match message {
                ProviderMessage::User { content } => Some(content.as_str()),
                _ => None,
            })
            .unwrap_or("");

        let response = format!("Echo: {prompt}");
        let delay = std::time::Duration::from_millis(self.delay_ms);
        for ch in response.chars() {
            if tx.send(ProviderEvent::TextDelta(ch.to_string())).is_err() {
                return;
            }
            if self.delay_ms > 0 {
                std::thread::sleep(delay);
            }
        }
        let _ = tx.send(ProviderEvent::TurnComplete);
    }
}
