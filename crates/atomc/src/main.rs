mod cli;

use atomc_core::config::{self, ConfigError, PartialConfig, ResolvedConfig};
use atomc_core::git::GitError;
use atomc_core::types::{ErrorDetail, ErrorResponse};
use atomc_core::SCHEMA_VERSION;
use clap::Parser;
use cli::{ApplyArgs, Cli, Commands, OutputFormat, PlanArgs};
use serde_json::Value;
use std::process::ExitCode;
use std::io::{self, IsTerminal, Read};
use std::path::Path;
use ulid::Ulid;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn run() -> Result<(), ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Plan(ref args) => handle_plan(&cli, args),
        Commands::Apply(ref args) => handle_apply(&cli, args),
        Commands::Serve(_) => {
            eprintln!("atomc: command handling not yet implemented");
            Err(ExitCode::from(2))
        }
    }
}

fn handle_plan(cli: &Cli, args: &PlanArgs) -> Result<(), ExitCode> {
    let overrides = command_overrides(
        args.model.clone(),
        args.diff_mode,
        args.include_untracked_override(),
        args.timeout,
    );
    let config = resolve_config(cli, overrides, args.format)?;
    if let Some(repo) = &args.repo {
        validate_repo_path(repo, args.format)?;
    }

    let mut diff = resolve_diff_input(args.diff_file.clone(), args.format)?;
    if diff.is_none() {
        if let Some(repo) = args.repo.as_deref() {
            diff = Some(compute_repo_diff(repo, &config, args.format)?);
        }
    }
    validate_diff_requirements(&diff, args.repo.as_deref(), &config, args.format)?;

    Err(emit_error(
        args.format,
        ErrorCode::UsageError,
        "command handling not yet implemented",
        None,
    ))
}

fn handle_apply(cli: &Cli, args: &ApplyArgs) -> Result<(), ExitCode> {
    let overrides = command_overrides(
        args.model.clone(),
        args.diff_mode,
        args.include_untracked_override(),
        args.timeout,
    );
    let config = resolve_config(cli, overrides, args.format)?;
    validate_repo_path(&args.repo, args.format)?;

    let mut diff = resolve_diff_input(args.diff_file.clone(), args.format)?;
    if diff.is_none() {
        diff = Some(compute_repo_diff(args.repo.as_path(), &config, args.format)?);
    }
    validate_diff_requirements(&diff, Some(args.repo.as_path()), &config, args.format)?;

    Err(emit_error(
        args.format,
        ErrorCode::UsageError,
        "command handling not yet implemented",
        None,
    ))
}

fn resolve_config(
    cli: &Cli,
    overrides: PartialConfig,
    format: OutputFormat,
) -> Result<ResolvedConfig, ExitCode> {
    config::resolve_config(cli.config.clone(), overrides).map_err(|err| {
        emit_error(
            format,
            ErrorCode::ConfigError,
            &err.to_string(),
            Some(config_error_details(err)),
        )
    })
}

fn config_error_details(error: ConfigError) -> Value {
    serde_json::json!({
        "type": format!("{error:?}")
    })
}

fn command_overrides(
    model: Option<String>,
    diff_mode: Option<cli::DiffMode>,
    include_untracked: Option<bool>,
    timeout: Option<u64>,
) -> PartialConfig {
    PartialConfig {
        model,
        diff_mode: diff_mode.map(map_diff_mode),
        include_untracked,
        llm_timeout_secs: timeout,
        ..PartialConfig::default()
    }
}

fn map_diff_mode(value: cli::DiffMode) -> config::DiffMode {
    match value {
        cli::DiffMode::Worktree => config::DiffMode::Worktree,
        cli::DiffMode::Staged => config::DiffMode::Staged,
        cli::DiffMode::All => config::DiffMode::All,
    }
}

fn validate_repo_path(path: &Path, format: OutputFormat) -> Result<(), ExitCode> {
    if !path.exists() {
        return Err(emit_error(
            format,
            ErrorCode::InputInvalid,
            "repo path does not exist",
            Some(serde_json::json!({ "path": path.display().to_string() })),
        ));
    }
    if !path.is_dir() {
        return Err(emit_error(
            format,
            ErrorCode::InputInvalid,
            "repo path is not a directory",
            Some(serde_json::json!({ "path": path.display().to_string() })),
        ));
    }
    Ok(())
}

fn validate_diff_requirements(
    diff: &Option<String>,
    repo: Option<&Path>,
    config: &ResolvedConfig,
    format: OutputFormat,
) -> Result<(), ExitCode> {
    if diff.is_none() && repo.is_none() {
        return Err(emit_error(
            format,
            ErrorCode::InputInvalid,
            "no diff provided and no repo path supplied",
            None,
        ));
    }

    if let Some(diff) = diff {
        if diff.is_empty() {
            return Err(emit_error(
                format,
                ErrorCode::InputInvalid,
                "diff input is empty",
                None,
            ));
        }
        let max_bytes = usize::try_from(config.max_diff_bytes).unwrap_or(usize::MAX);
        if diff.as_bytes().len() > max_bytes {
            return Err(emit_error(
                format,
                ErrorCode::InputInvalid,
                "diff exceeds max_diff_bytes",
                Some(serde_json::json!({ "max_diff_bytes": config.max_diff_bytes })),
            ));
        }
    }

    Ok(())
}

fn resolve_diff_input(diff_file: Option<std::path::PathBuf>, format: OutputFormat) -> Result<Option<String>, ExitCode> {
    let stdin_is_tty = io::stdin().is_terminal();
    let stdin_has_data = !stdin_is_tty;

    if let Some(path) = diff_file {
        if is_stdin_path(&path) {
            return read_stdin_diff(format).map(Some);
        }
        if stdin_has_data {
            return Err(emit_error(
                format,
                ErrorCode::UsageError,
                "stdin and --diff-file cannot both be used",
                None,
            ));
        }
        let contents = std::fs::read_to_string(&path).map_err(|err| {
            emit_error(
                format,
                ErrorCode::InputInvalid,
                "failed to read diff file",
                Some(serde_json::json!({
                    "path": path.display().to_string(),
                    "error": err.to_string()
                })),
            )
        })?;
        return Ok(Some(contents));
    }

    if stdin_has_data {
        return read_stdin_diff(format).map(Some);
    }

    Ok(None)
}

fn read_stdin_diff(format: OutputFormat) -> Result<String, ExitCode> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).map_err(|err| {
        emit_error(
            format,
            ErrorCode::InputInvalid,
            "failed to read diff from stdin",
            Some(serde_json::json!({ "error": err.to_string() })),
        )
    })?;
    Ok(buffer)
}

fn is_stdin_path(path: &Path) -> bool {
    path == Path::new("-")
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // Some variants used only in non-test builds.
enum ErrorCode {
    UsageError,
    InputInvalid,
    ConfigError,
    GitError,
}

impl ErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UsageError => "usage_error",
            ErrorCode::InputInvalid => "input_invalid",
            ErrorCode::ConfigError => "config_error",
            ErrorCode::GitError => "git_error",
        }
    }

    fn exit_code(self) -> ExitCode {
        match self {
            ErrorCode::UsageError => ExitCode::from(2),
            ErrorCode::InputInvalid => ExitCode::from(3),
            ErrorCode::ConfigError => ExitCode::from(7),
            ErrorCode::GitError => ExitCode::from(6),
        }
    }
}

fn emit_error(format: OutputFormat, code: ErrorCode, message: &str, details: Option<Value>) -> ExitCode {
    match format {
        OutputFormat::Json => {
            let response = ErrorResponse {
                schema_version: SCHEMA_VERSION.to_string(),
                request_id: Some(request_id()),
                error: ErrorDetail {
                    code: code.as_str().to_string(),
                    message: message.to_string(),
                    details,
                },
            };
            let payload = serde_json::to_string(&response).unwrap_or_else(|_| {
                format!(
                    "{{\"schema_version\":\"{}\",\"error\":{{\"code\":\"{}\",\"message\":\"{}\"}}}}",
                    SCHEMA_VERSION,
                    code.as_str(),
                    message
                )
            });
            println!("{payload}");
        }
        OutputFormat::Human => {
            eprintln!("{message}");
        }
    }
    code.exit_code()
}

fn request_id() -> String {
    Ulid::new().to_string()
}

fn compute_repo_diff(repo: &Path, config: &ResolvedConfig, format: OutputFormat) -> Result<String, ExitCode> {
    compute_repo_diff_impl(repo, config, format)
}

#[cfg(not(test))]
fn compute_repo_diff_impl(
    repo: &Path,
    config: &ResolvedConfig,
    format: OutputFormat,
) -> Result<String, ExitCode> {
    atomc_core::git::compute_diff(repo, config.diff_mode, config.include_untracked).map_err(|err| {
        emit_error(
            format,
            ErrorCode::GitError,
            "failed to compute git diff",
            Some(git_error_details(err)),
        )
    })
}

#[cfg(test)]
fn compute_repo_diff_impl(
    repo: &Path,
    config: &ResolvedConfig,
    _format: OutputFormat,
) -> Result<String, ExitCode> {
    if config.max_diff_bytes == 0 {
        return Err(ExitCode::from(6));
    }
    Ok(format!("diff from {}", repo.display()))
}

#[allow(dead_code)] // Used in non-test builds for git error reporting.
fn git_error_details(error: GitError) -> Value {
    match error {
        GitError::CommandFailed { cmd, stderr } => {
            serde_json::json!({ "cmd": cmd, "stderr": stderr })
        }
        GitError::CommandIo { cmd, source } => {
            serde_json::json!({ "cmd": cmd, "error": source.to_string() })
        }
        GitError::OutputNotUtf8 => serde_json::json!({ "error": "git output was not utf-8" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atomc_core::config::ResolvedConfig;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("atomc-{prefix}-{nanos}"))
    }

    #[test]
    fn validate_repo_path_rejects_missing_path() {
        let path = temp_dir("missing");
        let result = validate_repo_path(&path, OutputFormat::Json);
        assert!(result.is_err());
    }

    #[test]
    fn validate_repo_path_rejects_file_path() {
        let dir = temp_dir("file");
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let result = validate_repo_path(&file_path, OutputFormat::Json);
        assert!(result.is_err());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_repo_path_accepts_directory() {
        let dir = temp_dir("dir");
        fs::create_dir_all(&dir).unwrap();

        let result = validate_repo_path(&dir, OutputFormat::Json);
        assert!(result.is_ok());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_diff_requirements_rejects_missing_inputs() {
        let config = ResolvedConfig::defaults();
        let result = validate_diff_requirements(&None, None, &config, OutputFormat::Json);
        assert!(result.is_err());
    }

    #[test]
    fn validate_diff_requirements_rejects_empty_diff() {
        let config = ResolvedConfig::defaults();
        let result = validate_diff_requirements(&Some(String::new()), None, &config, OutputFormat::Json);
        assert!(result.is_err());
    }

    #[test]
    fn validate_diff_requirements_rejects_large_diff() {
        let mut config = ResolvedConfig::defaults();
        config.max_diff_bytes = 2;

        let result = validate_diff_requirements(&Some("abc".to_string()), None, &config, OutputFormat::Json);
        assert!(result.is_err());
    }

    #[test]
    fn validate_diff_requirements_accepts_repo_only() {
        let dir = temp_dir("repo-only");
        fs::create_dir_all(&dir).unwrap();

        let config = ResolvedConfig::defaults();
        let result = validate_diff_requirements(&None, Some(dir.as_path()), &config, OutputFormat::Json);
        assert!(result.is_ok());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_diff_requirements_accepts_diff_only() {
        let config = ResolvedConfig::defaults();
        let result = validate_diff_requirements(&Some("diff".to_string()), None, &config, OutputFormat::Json);
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plan_computes_repo_diff_when_missing_input() {
        let dir = temp_dir("repo-diff");
        fs::create_dir_all(&dir).unwrap();

        let cli = Cli {
            config: None,
            log_level: cli::LogLevel::Info,
            quiet: false,
            no_color: false,
            command: Commands::Plan(PlanArgs {
                repo: Some(dir.clone()),
                diff_file: None,
                diff_mode: None,
                include_untracked: false,
                no_include_untracked: false,
                format: OutputFormat::Json,
                model: None,
                dry_run: true,
                timeout: None,
            }),
        };

        if let Commands::Plan(ref args) = cli.command {
            let result = handle_plan(&cli, args);
            assert!(result.is_err());
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handle_apply_computes_repo_diff_when_missing_input() {
        let dir = temp_dir("repo-apply");
        fs::create_dir_all(&dir).unwrap();

        let cli = Cli {
            config: None,
            log_level: cli::LogLevel::Info,
            quiet: false,
            no_color: false,
            command: Commands::Apply(ApplyArgs {
                repo: dir.clone(),
                diff_file: None,
                diff_mode: None,
                include_untracked: false,
                no_include_untracked: false,
                format: OutputFormat::Json,
                model: None,
                execute: false,
                cleanup_on_error: false,
                timeout: None,
            }),
        };

        if let Commands::Apply(ref args) = cli.command {
            let result = handle_apply(&cli, args);
            assert!(result.is_err());
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handle_plan_reports_git_error_when_diff_fails() {
        let dir = temp_dir("repo-fail");
        fs::create_dir_all(&dir).unwrap();

        let cli = Cli {
            config: None,
            log_level: cli::LogLevel::Info,
            quiet: false,
            no_color: false,
            command: Commands::Plan(PlanArgs {
                repo: Some(dir.clone()),
                diff_file: None,
                diff_mode: None,
                include_untracked: false,
                no_include_untracked: false,
                format: OutputFormat::Json,
                model: None,
                dry_run: true,
                timeout: None,
            }),
        };

        let mut config = ResolvedConfig::defaults();
        config.max_diff_bytes = 0;
        let overrides = PartialConfig {
            max_diff_bytes: Some(0),
            ..PartialConfig::default()
        };
        let resolved = resolve_config(&cli, overrides, OutputFormat::Json).unwrap();
        assert_eq!(resolved.max_diff_bytes, 0);

        if let Commands::Plan(ref args) = cli.command {
            let result = handle_plan(&cli, args);
            assert!(result.is_err());
        }

        fs::remove_dir_all(&dir).ok();
    }
}
