use atomc_core::config::{resolve_config, PartialConfig, ResolvedConfig, Runtime};
use once_cell::sync::Lazy;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct EnvVarGuard {
    key: String,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(&self.key, value);
        } else {
            std::env::remove_var(&self.key);
        }
    }
}

fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("atomc-{prefix}-{nanos}"))
}

#[test]
fn resolve_config_defaults_from_empty_file() {
    let _lock = ENV_LOCK.lock().unwrap();
    let dir = temp_dir("config-defaults");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    fs::write(&path, "# empty config\n").unwrap();

    let resolved = resolve_config(Some(path), PartialConfig::default()).unwrap();
    assert_eq!(resolved.model, "deepseek-coder");
    assert_eq!(resolved.max_diff_bytes, 2_000_000);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_config_applies_precedence() {
    let _lock = ENV_LOCK.lock().unwrap();
    let dir = temp_dir("config-precedence");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    fs::write(&path, "model = \"file-model\"\nruntime = \"ollama\"\n").unwrap();

    let _env_model = EnvVarGuard::set("LOCAL_COMMIT_MODEL", "env-model");
    let _env_runtime = EnvVarGuard::set("LOCAL_COMMIT_RUNTIME", "llama.cpp");

    let overrides = PartialConfig {
        model: Some("cli-model".to_string()),
        ..PartialConfig::default()
    };

    let resolved = resolve_config(Some(path), overrides).unwrap();
    assert_eq!(resolved.model, "cli-model");
    assert_eq!(resolved.runtime, Runtime::LlamaCpp);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_config_rejects_invalid_env_values() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _env_tokens = EnvVarGuard::set("LOCAL_COMMIT_MAX_TOKENS", "nope");

    let result = resolve_config(None, PartialConfig::default());
    assert!(result.is_err());
}

#[test]
fn resolved_defaults_match_expected_values() {
    let defaults = ResolvedConfig::defaults();
    assert_eq!(defaults.ollama_url, "http://localhost:11434");
    assert_eq!(defaults.llm_timeout_secs, 60);
}
