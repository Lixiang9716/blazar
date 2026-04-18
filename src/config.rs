use serde_json::Value;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_SCHEMA_PATH: &str = "config/app.json";

pub fn load_app_schema() -> Result<Value, ConfigError> {
    load_app_schema_from_path(APP_SCHEMA_PATH)
}

pub fn load_app_schema_from_path(path: impl AsRef<Path>) -> Result<Value, ConfigError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let schema = serde_json::from_str(&contents).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    schema_title_from_path(&schema, path)?;

    Ok(schema)
}

pub fn schema_title(schema: &Value) -> Result<&str, ConfigError> {
    schema_title_from_path(schema, Path::new(APP_SCHEMA_PATH))
}

fn schema_title_from_path<'a>(schema: &'a Value, path: &Path) -> Result<&'a str, ConfigError> {
    schema["title"]
        .as_str()
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "schema title must be a string",
        })
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidSchema {
        path: PathBuf,
        message: &'static str,
    },
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read config file {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(
                    f,
                    "failed to parse config file {}: {source}",
                    path.display()
                )
            }
            Self::InvalidSchema { path, message } => {
                write!(f, "invalid config schema {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::InvalidSchema { .. } => None,
        }
    }
}
