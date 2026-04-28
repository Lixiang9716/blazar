//! Background model-metadata refresh subsystem.
//!
//! Periodically fetches the provider's model list and config on a background
//! thread so the UI can display context-length limits and populate the model
//! picker without blocking the event loop.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::provider::{ModelInfo, ProviderConfigPort};

#[derive(Debug, Clone)]
pub(super) struct ModelMetadataRefreshResult {
    pub(super) request_id: u64,
    pub(super) available_models: Vec<ModelInfo>,
    pub(super) config_max_tokens: Option<u32>,
}

pub(crate) struct ModelMetadataState {
    pub(super) available_models: Vec<ModelInfo>,
    pub(super) model_context_max_tokens: Option<u32>,
    pub(super) config_max_tokens: Option<u32>,
    provider_config: Arc<dyn ProviderConfigPort>,
    tx: Sender<ModelMetadataRefreshResult>,
    rx: Receiver<ModelMetadataRefreshResult>,
    handle: Option<std::thread::JoinHandle<()>>,
    stalled_handle: Option<std::thread::JoinHandle<()>>,
    pub(super) retry_exhausted: bool,
    in_flight: bool,
    pub(super) interval: Duration,
    pub(super) timeout: Duration,
    active_request_id: u64,
    last_refresh_at: Instant,
    started_at: Option<Instant>,
}

impl ModelMetadataState {
    pub(super) fn new(
        config_max_tokens: Option<u32>,
        provider_config: Arc<dyn ProviderConfigPort>,
    ) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            available_models: Vec::new(),
            model_context_max_tokens: None,
            config_max_tokens,
            provider_config,
            tx,
            rx,
            handle: None,
            stalled_handle: None,
            retry_exhausted: false,
            in_flight: false,
            interval: Duration::from_secs(300),
            timeout: Duration::from_secs(30),
            active_request_id: 0,
            last_refresh_at: Instant::now(),
            started_at: None,
        }
    }

    /// Called after the active model changes — refreshes cached limits and
    /// schedules a background fetch for fresh data.
    pub(super) fn on_model_changed(&mut self, workspace_root: &Path, model_name: &str) {
        let repo_root = workspace_root.to_string_lossy();
        self.config_max_tokens = self.provider_config.configured_max_tokens(&repo_root);
        self.model_context_max_tokens = self
            .provider_config
            .resolve_model_context_length(&self.available_models, model_name);
        self.schedule_refresh(workspace_root);
    }

    /// Per-tick driver: detects stale/timed-out refreshes and applies results.
    pub(super) fn tick(&mut self, workspace_root: &Path, model_name: &str) {
        self.expire_stale_refresh();
        self.apply_completed(model_name);

        if !self.in_flight && self.last_refresh_at.elapsed() >= self.interval {
            self.schedule_refresh(workspace_root);
        }
    }

    /// Resolved context-length limit: provider model limit or config override.
    pub(super) fn resolved_context_limit(&self) -> Option<u32> {
        self.model_context_max_tokens.or(self.config_max_tokens)
    }

    /// Live-fetch available models through the provider config port.
    pub(super) fn fetch_available_models(&self, repo_root: &str) -> Vec<ModelInfo> {
        self.provider_config.available_models(repo_root)
    }

    // ── private ─────────────────────────────────────────────────────

    fn schedule_refresh(&mut self, workspace_root: &Path) {
        if self.in_flight {
            return;
        }

        if self
            .stalled_handle
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            self.stalled_handle = None;
            self.retry_exhausted = false;
        }

        if self.stalled_handle.is_some() && self.retry_exhausted {
            return;
        }

        self.active_request_id = self.active_request_id.wrapping_add(1);
        self.in_flight = true;
        self.started_at = Some(Instant::now());
        let request_id = self.active_request_id;
        let tx = self.tx.clone();
        let repo_root = workspace_root.to_string_lossy().into_owned();
        #[cfg(test)]
        let available_models_behavior =
            crate::provider::current_available_models_behavior_for_test();
        self.handle = Some(std::thread::spawn(move || {
            #[cfg(test)]
            let _available_models_behavior =
                crate::provider::set_available_models_behavior_for_current_thread_for_test(
                    available_models_behavior,
                );
            let result = ModelMetadataRefreshResult {
                request_id,
                available_models: crate::provider::available_models(&repo_root),
                config_max_tokens: crate::provider::configured_max_tokens(&repo_root),
            };
            let _ = tx.send(result);
        }));
    }

    fn expire_stale_refresh(&mut self) {
        if !self.in_flight
            || self
                .started_at
                .is_none_or(|started_at| started_at.elapsed() < self.timeout)
        {
            return;
        }

        if self
            .handle
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            self.handle = None;
            self.in_flight = false;
            self.started_at = None;
        } else if self
            .stalled_handle
            .as_ref()
            .is_some_and(|h| !h.is_finished())
        {
            self.handle = None;
            self.in_flight = false;
            self.started_at = None;
            self.retry_exhausted = true;
            self.active_request_id = self.active_request_id.wrapping_add(1);
            self.last_refresh_at = Instant::now();
        } else {
            self.stalled_handle = self.handle.take();
            self.in_flight = false;
            self.started_at = None;
        }
    }

    fn apply_completed(&mut self, model_name: &str) {
        while let Ok(result) = self.rx.try_recv() {
            if result.request_id != self.active_request_id {
                continue;
            }
            self.available_models = result.available_models;
            self.config_max_tokens = result.config_max_tokens;
            self.model_context_max_tokens = self
                .provider_config
                .resolve_model_context_length(&self.available_models, model_name);
            self.handle = None;
            self.in_flight = false;
            self.started_at = None;
            self.retry_exhausted = false;
            self.last_refresh_at = Instant::now();
        }
    }
}
