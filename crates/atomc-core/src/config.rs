use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum Runtime {
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "llama.cpp")]
    LlamaCpp,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode {
    Worktree,
    Staged,
    All,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PartialConfig {
    pub model: Option<String>,
    pub runtime: Option<Runtime>,
    pub ollama_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub llm_timeout_secs: Option<u64>,
    pub max_diff_bytes: Option<u64>,
    pub diff_mode: Option<DiffMode>,
    pub include_untracked: Option<bool>,
    pub log_diff: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub model: String,
    pub runtime: Runtime,
    pub ollama_url: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub llm_timeout_secs: u64,
    pub max_diff_bytes: u64,
    pub diff_mode: DiffMode,
    pub include_untracked: bool,
    pub log_diff: bool,
}

impl ResolvedConfig {
    pub fn defaults() -> Self {
        Self {
            model: "deepseek-coder".to_string(),
            runtime: Runtime::Ollama,
            ollama_url: "http://localhost:11434".to_string(),
            max_tokens: 2048,
            temperature: 0.2,
            llm_timeout_secs: 60,
            max_diff_bytes: 2_000_000,
            diff_mode: DiffMode::All,
            include_untracked: true,
            log_diff: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found: {path}")]
    MissingFile { path: PathBuf },
    #[error("config file read error: {path}: {source}")]
    ReadFile { path: PathBuf, source: std::io::Error },
    #[error("config file parse error: {path}: {source}")]
    ParseFile { path: PathBuf, source: toml::de::Error },
    #[error("config path error: {0}")]
    Path(String),
    #[error("invalid env var {key}={value}")]
    InvalidEnv { key: String, value: String },
}

pub fn resolve_config(
    cli_path: Option<PathBuf>,
    overrides: PartialConfig,
) -> Result<ResolvedConfig, ConfigError> {
    let env_path = config_path_from_env();
    let required = cli_path.is_some() || env_path.is_some();
    let path = match cli_path.clone().or(env_path.clone()) {
        Some(path) => path,
        None => default_config_path()?,
    };

    let file_config = load_config_file(&path, required)?;
    let env_config = load_env_config()?;

    let mut resolved = ResolvedConfig::defaults();
    // Precedence: defaults < config file < env vars < CLI overrides.
    file_config.apply_to(&mut resolved);
    env_config.apply_to(&mut resolved);
    overrides.apply_to(&mut resolved);

    Ok(resolved)
}

fn load_config_file(path: &Path, required: bool) -> Result<PartialConfig, ConfigError> {
    if !path.exists() {
        if required {
            return Err(ConfigError::MissingFile {
                path: path.to_path_buf(),
            });
        }
        return Ok(PartialConfig::default());
    }

    let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;

    let config = toml::from_str(&contents).map_err(|source| ConfigError::ParseFile {
        path: path.to_path_buf(),
        source,
    })?;

    Ok(config)
}

fn load_env_config() -> Result<PartialConfig, ConfigError> {
    let mut config = PartialConfig::default();

    if let Some(value) = env("LOCAL_COMMIT_MODEL") {
        config.model = Some(value);
    }
    if let Some(value) = env("LOCAL_COMMIT_RUNTIME") {
        config.runtime = Some(parse_runtime("LOCAL_COMMIT_RUNTIME", &value)?);
    }
    if let Some(value) = env("LOCAL_COMMIT_OLLAMA_URL") {
        config.ollama_url = Some(value);
    }
    if let Some(value) = env("LOCAL_COMMIT_MAX_TOKENS") {
        config.max_tokens = Some(parse_u32("LOCAL_COMMIT_MAX_TOKENS", &value)?);
    }
    if let Some(value) = env("LOCAL_COMMIT_TEMPERATURE") {
        config.temperature = Some(parse_f32("LOCAL_COMMIT_TEMPERATURE", &value)?);
    }
    if let Some(value) = env("LOCAL_COMMIT_LLM_TIMEOUT_SECS") {
        config.llm_timeout_secs = Some(parse_u64("LOCAL_COMMIT_LLM_TIMEOUT_SECS", &value)?);
    }
    if let Some(value) = env("LOCAL_COMMIT_MAX_DIFF_BYTES") {
        config.max_diff_bytes = Some(parse_u64("LOCAL_COMMIT_MAX_DIFF_BYTES", &value)?);
    }
    if let Some(value) = env("LOCAL_COMMIT_DIFF_MODE") {
        config.diff_mode = Some(parse_diff_mode("LOCAL_COMMIT_DIFF_MODE", &value)?);
    }
    if let Some(value) = env("LOCAL_COMMIT_INCLUDE_UNTRACKED") {
        config.include_untracked = Some(parse_bool("LOCAL_COMMIT_INCLUDE_UNTRACKED", &value)?);
    }
    if let Some(value) = env("LOCAL_COMMIT_LOG_DIFF") {
        config.log_diff = Some(parse_bool("LOCAL_COMMIT_LOG_DIFF", &value)?);
    }

    Ok(config)
}

fn config_path_from_env() -> Option<PathBuf> {
    env_os("LOCAL_COMMIT_AGENT_CONFIG").map(PathBuf::from)
}

fn default_config_path() -> Result<PathBuf, ConfigError> {
    let base_dirs = directories::BaseDirs::new()
        .ok_or_else(|| ConfigError::Path("home directory not available".to_string()))?;

    if cfg!(target_os = "macos") {
        Ok(base_dirs
            .home_dir()
            .join("Library/Application Support/atomc/config.toml"))
    } else {
        Ok(base_dirs.home_dir().join(".config/atomc/config.toml"))
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn env_os(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

fn parse_runtime(key: &str, value: &str) -> Result<Runtime, ConfigError> {
    match value {
        "ollama" => Ok(Runtime::Ollama),
        "llama.cpp" | "llama_cpp" | "llamacpp" => Ok(Runtime::LlamaCpp),
        _ => Err(ConfigError::InvalidEnv {
            key: key.to_string(),
            value: value.to_string(),
        }),
    }
}

fn parse_diff_mode(key: &str, value: &str) -> Result<DiffMode, ConfigError> {
    match value {
        "worktree" => Ok(DiffMode::Worktree),
        "staged" => Ok(DiffMode::Staged),
        "all" => Ok(DiffMode::All),
        _ => Err(ConfigError::InvalidEnv {
            key: key.to_string(),
            value: value.to_string(),
        }),
    }
}

fn parse_bool(key: &str, value: &str) -> Result<bool, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        _ => Err(ConfigError::InvalidEnv {
            key: key.to_string(),
            value: value.to_string(),
        }),
    }
}

fn parse_u32(key: &str, value: &str) -> Result<u32, ConfigError> {
    value.parse().map_err(|_| ConfigError::InvalidEnv {
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn parse_u64(key: &str, value: &str) -> Result<u64, ConfigError> {
    value.parse().map_err(|_| ConfigError::InvalidEnv {
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn parse_f32(key: &str, value: &str) -> Result<f32, ConfigError> {
    value.parse().map_err(|_| ConfigError::InvalidEnv {
        key: key.to_string(),
        value: value.to_string(),
    })
}

impl PartialConfig {
    fn apply_to(self, resolved: &mut ResolvedConfig) {
        if let Some(value) = self.model {
            resolved.model = value;
        }
        if let Some(value) = self.runtime {
            resolved.runtime = value;
        }
        if let Some(value) = self.ollama_url {
            resolved.ollama_url = value;
        }
        if let Some(value) = self.max_tokens {
            resolved.max_tokens = value;
        }
        if let Some(value) = self.temperature {
            resolved.temperature = value;
        }
        if let Some(value) = self.llm_timeout_secs {
            resolved.llm_timeout_secs = value;
        }
        if let Some(value) = self.max_diff_bytes {
            resolved.max_diff_bytes = value;
        }
        if let Some(value) = self.diff_mode {
            resolved.diff_mode = value;
        }
        if let Some(value) = self.include_untracked {
            resolved.include_untracked = value;
        }
        if let Some(value) = self.log_diff {
            resolved.log_diff = value;
        }
    }
}
