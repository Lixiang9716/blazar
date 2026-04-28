use crate::chat;
use crate::config;
use serde_json::Value;

pub(crate) fn build_schema() -> Result<Value, config::ConfigError> {
    config::load_app_schema()
}

pub(crate) type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn runtime_name_for_test() -> &'static str {
    "spirit-chat-tui"
}

pub fn run() -> AppResult<()> {
    init_tokio_console();
    init_logger();
    log::info!("Blazar starting");
    let schema = build_schema()?;
    chat::event_loop::run_terminal_chat(schema)
}

/// Initialize file-based logger.  Logs go to `logs/blazar.log` in the repo
/// root.  The TUI owns stdout/stderr so all logging must go to a file.
fn init_logger() {
    use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, Naming, WriteMode};

    let log_dir = if cfg!(test) {
        std::env::current_dir()
            .unwrap_or_default()
            .join("target")
            .join("test-logs")
    } else {
        std::env::current_dir().unwrap_or_default().join("logs")
    };
    let _ = std::fs::create_dir_all(&log_dir);

    // Default: blazar=debug, suppress noisy third-party crates.
    // Override with BLAZAR_LOG env var (e.g. "trace" or "blazar=trace,ureq=debug").
    let level = std::env::var("BLAZAR_LOG").unwrap_or_else(|_| {
        "warn, blazar=debug, ureq=warn, rustls=warn, hyper=warn, h2=warn".to_owned()
    });

    if let Err(e) = Logger::try_with_str(&level).and_then(|logger| {
        logger
            .log_to_file(
                FileSpec::default()
                    .directory(log_dir)
                    .basename("blazar")
                    .suppress_timestamp(),
            )
            .rotate(
                Criterion::Size(5_000_000), // rotate at 5 MB
                Naming::Numbers,
                Cleanup::KeepLogFiles(3),
            )
            .write_mode(WriteMode::BufferAndFlush)
            .format(crate::observability::logging::flexi_structured_format)
            .start()
    }) {
        eprintln!("Failed to init logger: {e}");
    }
}

fn init_tokio_console() {
    #[cfg(feature = "tokio-console")]
    if std::env::var_os("BLAZAR_ENABLE_TOKIO_CONSOLE").is_some() {
        console_subscriber::init();
    }
}

#[cfg(test)]
#[path = "../tests/unit/app_tests.rs"]
mod tests;
