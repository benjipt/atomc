mod cli;

use atomc_core::config::{self, ConfigError, PartialConfig, ResolvedConfig};
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

    let diff = resolve_diff_input(args.diff_file.clone(), args.format)?;
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

    let diff = resolve_diff_input(args.diff_file.clone(), args.format)?;
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
enum ErrorCode {
    UsageError,
    InputInvalid,
    ConfigError,
}

impl ErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UsageError => "usage_error",
            ErrorCode::InputInvalid => "input_invalid",
            ErrorCode::ConfigError => "config_error",
        }
    }

    fn exit_code(self) -> ExitCode {
        match self {
            ErrorCode::UsageError => ExitCode::from(2),
            ErrorCode::InputInvalid => ExitCode::from(3),
            ErrorCode::ConfigError => ExitCode::from(7),
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
