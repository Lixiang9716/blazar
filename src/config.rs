use serde_json::Value;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_SCHEMA_PATH: &str = "config/app.json";

pub fn load_app_schema() -> Result<Value, ConfigError> {
    load_app_schema_from_path(APP_SCHEMA_PATH)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MascotConfig {
    pub asset_path: String,
    pub frame_count: u32,
    pub fps: u16,
}

impl MascotConfig {
    pub fn frame_interval_ms(&self) -> u64 {
        1_000 / u64::from(self.fps)
    }
}

pub fn load_mascot_config() -> Result<MascotConfig, ConfigError> {
    load_mascot_config_from_path(APP_SCHEMA_PATH)
}

pub fn load_mascot_config_from_path(path: impl AsRef<Path>) -> Result<MascotConfig, ConfigError> {
    let path = path.as_ref();
    let schema = load_app_schema_from_path(path)?;

    let asset_path = schema["mascot"]["assetPath"]
        .as_str()
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "mascot.assetPath must be a string",
        })?
        .to_owned();
    let frame_count = schema["mascot"]["frameCount"]
        .as_u64()
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "mascot.frameCount must be a positive integer",
        })
        .and_then(|value| {
            u32::try_from(value).map_err(|_| ConfigError::InvalidSchema {
                path: path.to_path_buf(),
                message: "mascot.frameCount must fit within u32",
            })
        })?;
    let fps = schema["mascot"]["fps"]
        .as_u64()
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "mascot.fps must be a positive integer",
        })
        .and_then(|value| {
            u16::try_from(value).map_err(|_| ConfigError::InvalidSchema {
                path: path.to_path_buf(),
                message: "mascot.fps must fit within u16",
            })
        })?;

    if frame_count == 0 {
        return Err(ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "mascot.frameCount must be greater than 0",
        });
    }

    if fps == 0 {
        return Err(ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "mascot.fps must be greater than 0",
        });
    }

    Ok(MascotConfig {
        asset_path,
        frame_count,
        fps,
    })
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
