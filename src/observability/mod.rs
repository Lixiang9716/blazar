pub mod debug;
pub mod logging;

use debug::DebugEventSnapshot;

/// Port abstraction for observability recording.
///
/// Decouples ChatApp from the concrete `DebugRecorder` so that tests and
/// alternative backends can be substituted without modifying the chat layer.
pub trait ObservabilityPort: Send {
    fn start_turn(&mut self, turn_id: &str, turn_kind: Option<&str>, queue_depth: usize);

    fn record_event(
        &mut self,
        event_name: &str,
        tool_name: Option<&str>,
        call_id: Option<&str>,
        error_kind: Option<&str>,
        queue_depth: usize,
        message: &str,
    ) -> DebugEventSnapshot;

    fn finish_turn(&mut self, status: &str, error_kind: Option<&str>, error_message: Option<&str>);

    fn latest_turn_bundle(&self) -> Option<String>;

    fn status_summary(&self, pending_count: usize) -> String;
}
