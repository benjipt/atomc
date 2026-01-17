mod cli;

use atomc_core::config::{self, ConfigError, PartialConfig, ResolvedConfig};
use atomc_core::git::{self, GitError};
use atomc_core::hash;
use atomc_core::llm::{self, LlmError, PromptContext};
use atomc_core::semantic::{self, ScopePolicy, SemanticWarning};
use atomc_core::types::{
    ApplyResult, ApplyStatus, CommitApplyResponse, CommitPlan, DiffMode as OutputDiffMode,
    ErrorDetail, ErrorResponse, InputMeta, InputSource, Warning,
};
use atomc_core::SCHEMA_VERSION;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Json;
use axum::Router;
use clap::Parser;
use cli::{ApplyArgs, Cli, Commands, OutputFormat, PlanArgs, ServeArgs};
use serde::Deserialize;
use serde_json::Value;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tokio::net::TcpListener;
use ulid::Ulid;

#[cfg(test)]
static APPLY_SHOULD_FAIL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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
        Commands::Serve(ref args) => handle_serve(&cli, args),
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

    let mut diff = resolve_diff_input(args.diff_file.clone(), config.max_diff_bytes, args.format)?;
    let mut source = InputSource::Diff;
    if diff.is_none() {
        if let Some(repo) = args.repo.as_deref() {
            diff = Some(compute_repo_diff(repo, &config, args.format)?);
            source = InputSource::Repo;
        }
    }
    validate_diff_requirements(&diff, args.repo.as_deref(), &config, args.format)?;

    let diff = diff.ok_or_else(|| {
        emit_error(
            args.format,
            ErrorCode::InputInvalid,
            "diff input is missing",
            None,
        )
    })?;

    let prompt = llm::build_prompt(PromptContext {
        repo_path: args.repo.as_deref(),
        diff_mode: input_diff_mode(&source, config.diff_mode),
        include_untracked: input_include_untracked(&source, config.include_untracked),
        git_status: None,
        diff: &diff,
    });

    let mut plan = request_commit_plan(&config, &prompt, args.format)?;
    let warnings = apply_semantic_validation(&plan, args.format)?;
    plan.schema_version = SCHEMA_VERSION.to_string();
    plan.request_id = Some(request_id());
    plan.input = Some(build_input_meta(source.clone(), &config, &diff));
    plan.warnings = merge_warnings(plan.warnings.take(), warnings);

    emit_plan(args.format, &plan)
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

    let mut diff = resolve_diff_input(args.diff_file.clone(), config.max_diff_bytes, args.format)?;
    let mut source = InputSource::Diff;
    if diff.is_none() {
        diff = Some(compute_repo_diff(args.repo.as_path(), &config, args.format)?);
        source = InputSource::Repo;
    }
    validate_diff_requirements(&diff, Some(args.repo.as_path()), &config, args.format)?;

    let diff = diff.ok_or_else(|| {
        emit_error(
            args.format,
            ErrorCode::InputInvalid,
            "diff input is missing",
            None,
        )
    })?;

    let prompt = llm::build_prompt(PromptContext {
        repo_path: Some(args.repo.as_path()),
        diff_mode: input_diff_mode(&source, config.diff_mode),
        include_untracked: input_include_untracked(&source, config.include_untracked),
        git_status: None,
        diff: &diff,
    });

    let mut plan = request_commit_plan(&config, &prompt, args.format)?;
    let warnings = apply_semantic_validation(&plan, args.format)?;
    plan.schema_version = SCHEMA_VERSION.to_string();
    plan.request_id = Some(request_id());
    plan.input = Some(build_input_meta(source.clone(), &config, &diff));
    plan.warnings = merge_warnings(plan.warnings.take(), warnings.clone());

    let results = if args.execute {
        let request = git::ApplyRequest {
            repo: args.repo.as_path(),
            plan: &plan.plan,
            diff: &diff,
            diff_mode: config.diff_mode,
            include_untracked: config.include_untracked,
            expected_diff_hash: plan.input.as_ref().and_then(|input| input.diff_hash.clone()),
            cleanup_on_error: args.cleanup_on_error,
        };
        execute_apply_plan(request).map_err(|err| {
            emit_error(
                args.format,
                ErrorCode::GitError,
                "apply execution failed",
                Some(git_error_details(err)),
            )
        })?
    } else {
        planned_results(&plan)
    };

    let response = build_apply_response(plan, results, source, &config, &diff);

    emit_apply(args.format, &response)
}

fn handle_serve(cli: &Cli, args: &ServeArgs) -> Result<(), ExitCode> {
    let overrides = PartialConfig {
        model: args.model.clone(),
        llm_timeout_secs: Some(args.request_timeout),
        ..PartialConfig::default()
    };
    let config = resolve_config(cli, overrides, OutputFormat::Human)?;
    let state = ServerState { config };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            eprintln!("atomc: failed to start runtime: {err}");
            ExitCode::from(2)
        })?;

    runtime
        .block_on(run_server(args, state))
        .map_err(|err| {
            eprintln!("atomc: server error: {err}");
            ExitCode::from(2)
        })
}

#[derive(Clone)]
struct ServerState {
    config: ResolvedConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct PlanRequest {
    repo_path: Option<PathBuf>,
    diff: Option<String>,
    diff_mode: Option<config::DiffMode>,
    include_untracked: Option<bool>,
    git_status: Option<String>,
    model: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct ApplyRequestBody {
    repo_path: PathBuf,
    diff: Option<String>,
    diff_mode: Option<config::DiffMode>,
    include_untracked: Option<bool>,
    git_status: Option<String>,
    model: Option<String>,
    execute: Option<bool>,
    cleanup_on_error: Option<bool>,
    dry_run: Option<bool>,
}

async fn run_server(
    args: &ServeArgs,
    state: ServerState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/v1/commit-plan", post(plan_handler))
        .route("/v1/commit-apply", post(apply_handler))
        .with_state(state);

    let addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&addr).await?;
    println!("atomc: server listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn plan_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<PlanRequest>,
) -> Response {
    let request_id = extract_request_id(&headers);
    let config = config_with_request_overrides(
        &state.config,
        payload.model.clone(),
        payload.diff_mode,
        payload.include_untracked,
    );

    let repo_path = payload.repo_path.as_deref();
    if let Some(path) = repo_path {
        if let Err(response) = validate_repo_path_http(path, &request_id) {
            return response;
        }
    }

    let (diff, source) = match resolve_request_diff(
        repo_path,
        payload.diff,
        &config,
        &request_id,
    ) {
        Ok(result) => result,
        Err(response) => return response,
    };

    if let Err(response) = validate_diff_size(&diff, config.max_diff_bytes, &request_id) {
        return response;
    }

    let prompt = llm::build_prompt(PromptContext {
        repo_path,
        diff_mode: input_diff_mode(&source, config.diff_mode),
        include_untracked: input_include_untracked(&source, config.include_untracked),
        git_status: payload.git_status.as_deref(),
        diff: &diff,
    });

    let mut plan = match request_commit_plan_http(&config, &prompt, &request_id).await {
        Ok(plan) => plan,
        Err(response) => return response,
    };
    let warnings = match semantic_warnings_http(&plan, &request_id) {
        Ok(warnings) => warnings,
        Err(response) => return response,
    };

    plan.schema_version = SCHEMA_VERSION.to_string();
    plan.request_id = Some(request_id.clone());
    plan.input = Some(build_input_meta(source, &config, &diff));
    plan.warnings = merge_warnings(plan.warnings.take(), warnings);

    json_response(StatusCode::OK, &request_id, plan)
}

async fn apply_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<ApplyRequestBody>,
) -> Response {
    let request_id = extract_request_id(&headers);
    let config = config_with_request_overrides(
        &state.config,
        payload.model.clone(),
        payload.diff_mode,
        payload.include_untracked,
    );

    if let Err(response) = validate_repo_path_http(&payload.repo_path, &request_id) {
        return response;
    }

    let (diff, source) = match resolve_request_diff(
        Some(payload.repo_path.as_path()),
        payload.diff,
        &config,
        &request_id,
    ) {
        Ok(result) => result,
        Err(response) => return response,
    };

    if let Err(response) = validate_diff_size(&diff, config.max_diff_bytes, &request_id) {
        return response;
    }

    let prompt = llm::build_prompt(PromptContext {
        repo_path: Some(payload.repo_path.as_path()),
        diff_mode: input_diff_mode(&source, config.diff_mode),
        include_untracked: input_include_untracked(&source, config.include_untracked),
        git_status: payload.git_status.as_deref(),
        diff: &diff,
    });

    let mut plan = match request_commit_plan_http(&config, &prompt, &request_id).await {
        Ok(plan) => plan,
        Err(response) => return response,
    };
    let warnings = match semantic_warnings_http(&plan, &request_id) {
        Ok(warnings) => warnings,
        Err(response) => return response,
    };

    plan.schema_version = SCHEMA_VERSION.to_string();
    plan.request_id = Some(request_id.clone());
    plan.input = Some(build_input_meta(source.clone(), &config, &diff));
    plan.warnings = merge_warnings(plan.warnings.take(), warnings);

    let execute = payload.execute.unwrap_or(false);
    let dry_run = payload.dry_run.unwrap_or(false);
    let should_execute = execute && !dry_run;
    let cleanup_on_error = payload.cleanup_on_error.unwrap_or(false);

    let results = if should_execute {
        let request = git::ApplyRequest {
            repo: payload.repo_path.as_path(),
            plan: &plan.plan,
            diff: &diff,
            diff_mode: config.diff_mode,
            include_untracked: config.include_untracked,
            expected_diff_hash: plan.input.as_ref().and_then(|input| input.diff_hash.clone()),
            cleanup_on_error,
        };
        match git::apply_plan(request) {
            Ok(results) => results,
            Err(err) => {
                return error_response(
                    ErrorCode::GitError,
                    "apply execution failed",
                    Some(git_error_details(err)),
                    &request_id,
                )
            }
        }
    } else {
        planned_results(&plan)
    };

    let response = build_apply_response(plan, results, source, &config, &diff);
    json_response(StatusCode::OK, &request_id, response)
}

fn config_with_request_overrides(
    base: &ResolvedConfig,
    model: Option<String>,
    diff_mode: Option<config::DiffMode>,
    include_untracked: Option<bool>,
) -> ResolvedConfig {
    let mut config = base.clone();
    if let Some(model) = model {
        config.model = model;
    }
    if let Some(diff_mode) = diff_mode {
        config.diff_mode = diff_mode;
    }
    if let Some(include_untracked) = include_untracked {
        config.include_untracked = include_untracked;
    }
    config
}

fn resolve_request_diff(
    repo_path: Option<&Path>,
    diff: Option<String>,
    config: &ResolvedConfig,
    request_id: &str,
) -> Result<(String, InputSource), Response> {
    if let Some(diff) = diff {
        if diff.is_empty() {
            return Err(error_response(
                ErrorCode::InputInvalid,
                "diff input is empty",
                None,
                request_id,
            ));
        }
        return Ok((diff, InputSource::Diff));
    }

    let repo = repo_path.ok_or_else(|| {
        error_response(
            ErrorCode::InputInvalid,
            "no diff provided and no repo path supplied",
            None,
            request_id,
        )
    })?;

    let diff = git::compute_diff(repo, config.diff_mode, config.include_untracked).map_err(|err| {
        error_response(
            ErrorCode::GitError,
            "failed to compute git diff",
            Some(git_error_details(err)),
            request_id,
        )
    })?;

    if diff.is_empty() {
        return Err(error_response(
            ErrorCode::InputInvalid,
            "diff input is empty",
            None,
            request_id,
        ));
    }

    Ok((diff, InputSource::Repo))
}

fn validate_repo_path_http(path: &Path, request_id: &str) -> Result<(), Response> {
    if !path.exists() {
        return Err(error_response(
            ErrorCode::InputInvalid,
            "repo path does not exist",
            Some(serde_json::json!({ "path": path.display().to_string() })),
            request_id,
        ));
    }
    if !path.is_dir() {
        return Err(error_response(
            ErrorCode::InputInvalid,
            "repo path is not a directory",
            Some(serde_json::json!({ "path": path.display().to_string() })),
            request_id,
        ));
    }
    Ok(())
}

fn validate_diff_size(diff: &str, max_bytes: u64, request_id: &str) -> Result<(), Response> {
    let max_bytes_usize = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    if diff.as_bytes().len() > max_bytes_usize {
        return Err(error_response(
            ErrorCode::InputInvalid,
            "diff exceeds max_diff_bytes",
            Some(serde_json::json!({ "max_diff_bytes": max_bytes })),
            request_id,
        ));
    }
    Ok(())
}

async fn request_commit_plan_http(
    config: &ResolvedConfig,
    prompt: &llm::Prompt,
    request_id: &str,
) -> Result<CommitPlan, Response> {
    llm::generate_commit_plan(config, prompt)
        .await
        .map_err(|err| llm_error_response(err, request_id))
}

fn semantic_warnings_http(plan: &CommitPlan, request_id: &str) -> Result<Vec<Warning>, Response> {
    match semantic::validate_commit_units(&plan.plan, ScopePolicy::Warn) {
        Ok(report) => Ok(semantic_warnings_to_warnings(&report.warnings)),
        Err(errors) => {
            let details = serde_json::json!({
                "errors": errors.iter().map(|err| err.to_string()).collect::<Vec<_>>()
            });
            Err(error_response(
                ErrorCode::LlmParseError,
                "semantic validation failed",
                Some(details),
                request_id,
            ))
        }
    }
}

fn llm_error_response(error: LlmError, request_id: &str) -> Response {
    match error {
        LlmError::Runtime(message) => error_response(
            ErrorCode::LlmRuntimeError,
            "llm request failed",
            Some(serde_json::json!({ "error": message })),
            request_id,
        ),
        LlmError::Parse(message) => error_response(
            ErrorCode::LlmParseError,
            "llm response parse failed",
            Some(serde_json::json!({ "error": message })),
            request_id,
        ),
        LlmError::Timeout => {
            error_response(ErrorCode::Timeout, "llm request timed out", None, request_id)
        }
        LlmError::UnsupportedRuntime(runtime) => error_response(
            ErrorCode::ConfigError,
            "unsupported llm runtime",
            Some(serde_json::json!({ "runtime": runtime })),
            request_id,
        ),
    }
}

fn extract_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(request_id)
}

fn json_response<T: serde::Serialize>(status: StatusCode, request_id: &str, payload: T) -> Response {
    let mut headers = HeaderMap::new();
    let header_value = HeaderValue::from_str(request_id)
        .unwrap_or_else(|_| HeaderValue::from_static("invalid-request-id"));
    headers.insert("x-request-id", header_value);
    (status, headers, Json(payload)).into_response()
}

fn error_response(
    code: ErrorCode,
    message: &str,
    details: Option<Value>,
    request_id: &str,
) -> Response {
    let response = ErrorResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        request_id: Some(request_id.to_string()),
        error: ErrorDetail {
            code: code.as_str().to_string(),
            message: message.to_string(),
            details,
        },
    };
    json_response(status_for_error(code), request_id, response)
}

fn status_for_error(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::UsageError | ErrorCode::InputInvalid => StatusCode::BAD_REQUEST,
        ErrorCode::LlmRuntimeError | ErrorCode::LlmParseError => StatusCode::BAD_GATEWAY,
        ErrorCode::Timeout => StatusCode::GATEWAY_TIMEOUT,
        ErrorCode::GitError | ErrorCode::ConfigError => StatusCode::INTERNAL_SERVER_ERROR,
    }
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

fn input_diff_mode(source: &InputSource, mode: config::DiffMode) -> Option<config::DiffMode> {
    match source {
        InputSource::Repo => Some(mode),
        InputSource::Diff => None,
    }
}

fn input_include_untracked(source: &InputSource, include_untracked: bool) -> Option<bool> {
    match source {
        InputSource::Repo => Some(include_untracked),
        InputSource::Diff => None,
    }
}

fn output_diff_mode(mode: config::DiffMode) -> OutputDiffMode {
    match mode {
        config::DiffMode::Worktree => OutputDiffMode::Worktree,
        config::DiffMode::Staged => OutputDiffMode::Staged,
        config::DiffMode::All => OutputDiffMode::All,
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

fn build_input_meta(source: InputSource, config: &ResolvedConfig, diff: &str) -> InputMeta {
    let (diff_mode, include_untracked) = match source {
        InputSource::Repo => (
            Some(output_diff_mode(config.diff_mode)),
            Some(config.include_untracked),
        ),
        InputSource::Diff => (None, None),
    };

    InputMeta {
        source,
        diff_mode,
        include_untracked,
        diff_hash: Some(hash::diff_hash(diff)),
    }
}

fn apply_semantic_validation(plan: &CommitPlan, format: OutputFormat) -> Result<Vec<Warning>, ExitCode> {
    match semantic::validate_commit_units(&plan.plan, ScopePolicy::Warn) {
        Ok(report) => Ok(semantic_warnings_to_warnings(&report.warnings)),
        Err(errors) => {
            let details = serde_json::json!({
                "errors": errors.iter().map(|err| err.to_string()).collect::<Vec<_>>()
            });
            Err(emit_error(
                format,
                ErrorCode::LlmParseError,
                "semantic validation failed",
                Some(details),
            ))
        }
    }
}

fn build_apply_response(
    plan: CommitPlan,
    results: Vec<ApplyResult>,
    source: InputSource,
    config: &ResolvedConfig,
    diff: &str,
) -> CommitApplyResponse {
    let request_id = plan.request_id.clone().or_else(|| Some(request_id()));

    CommitApplyResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        request_id,
        warnings: plan.warnings,
        input: Some(build_input_meta(source, config, diff)),
        plan: plan.plan,
        results,
    }
}

fn planned_results(plan: &CommitPlan) -> Vec<ApplyResult> {
    plan.plan
        .iter()
        .map(|unit| ApplyResult {
            id: unit.id.clone(),
            status: ApplyStatus::Planned,
            commit_hash: None,
            error: None,
        })
        .collect()
}

fn applied_results(plan: &[atomc_core::types::CommitUnit]) -> Vec<ApplyResult> {
    plan.iter()
        .map(|unit| ApplyResult {
            id: unit.id.clone(),
            status: ApplyStatus::Applied,
            commit_hash: Some("test-hash".to_string()),
            error: None,
        })
        .collect()
}

fn semantic_warnings_to_warnings(warnings: &[SemanticWarning]) -> Vec<Warning> {
    warnings
        .iter()
        .map(|warning| match warning {
            SemanticWarning::ScopeMissing { id } => Warning {
                code: "scope_missing".to_string(),
                message: format!("commit {id} scope is missing"),
                details: None,
            },
        })
        .collect()
}

fn merge_warnings(existing: Option<Vec<Warning>>, new: Vec<Warning>) -> Option<Vec<Warning>> {
    let mut combined = existing.unwrap_or_default();
    combined.extend(new);
    if combined.is_empty() {
        None
    } else {
        Some(combined)
    }
}

fn resolve_diff_input(
    diff_file: Option<PathBuf>,
    max_bytes: u64,
    format: OutputFormat,
) -> Result<Option<String>, ExitCode> {
    let stdin_is_tty = io::stdin().is_terminal();
    // If stdin isn't a TTY, assume data is being piped.
    let stdin_has_data = !stdin_is_tty;

    if let Some(path) = diff_file {
        if is_stdin_path(&path) {
            return read_stdin_diff(max_bytes, format).map(Some);
        }
        if stdin_has_data {
            return Err(emit_error(
                format,
                ErrorCode::UsageError,
                "stdin and --diff-file cannot both be used",
                None,
            ));
        }
        let contents = read_diff_file(&path, max_bytes, format)?;
        return Ok(Some(contents));
    }

    if stdin_has_data {
        return read_stdin_diff(max_bytes, format).map(Some);
    }

    Ok(None)
}

fn read_diff_file(path: &Path, max_bytes: u64, format: OutputFormat) -> Result<String, ExitCode> {
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() > max_bytes {
            return Err(emit_error(
                format,
                ErrorCode::InputInvalid,
                "diff exceeds max_diff_bytes",
                Some(serde_json::json!({
                    "path": path.display().to_string(),
                    "max_diff_bytes": max_bytes
                })),
            ));
        }
    }

    let mut file = std::fs::File::open(path).map_err(|err| {
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

    read_limited(
        &mut file,
        max_bytes,
        format,
        "failed to read diff file",
        Some(serde_json::json!({ "path": path.display().to_string() })),
    )
}

fn read_stdin_diff(max_bytes: u64, format: OutputFormat) -> Result<String, ExitCode> {
    let mut stdin = io::stdin();
    read_limited(
        &mut stdin,
        max_bytes,
        format,
        "failed to read diff from stdin",
        None,
    )
}

fn read_limited<R: Read>(
    reader: &mut R,
    max_bytes: u64,
    format: OutputFormat,
    message: &str,
    details: Option<Value>,
) -> Result<String, ExitCode> {
    let mut buffer = String::new();
    let limit = max_bytes.saturating_add(1);
    // Read with a hard cap to avoid unbounded memory usage.
    let mut limited = reader.take(limit);
    let base_details = details.unwrap_or_else(|| serde_json::json!({}));
    limited.read_to_string(&mut buffer).map_err(|err| {
        let mut payload = base_details.clone();
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("error".to_string(), serde_json::json!(err.to_string()));
        }
        emit_error(format, ErrorCode::InputInvalid, message, Some(payload))
    })?;

    let max_bytes_usize = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    if buffer.as_bytes().len() > max_bytes_usize {
        let mut payload = base_details;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("max_diff_bytes".to_string(), serde_json::json!(max_bytes));
        }
        return Err(emit_error(
            format,
            ErrorCode::InputInvalid,
            "diff exceeds max_diff_bytes",
            Some(payload),
        ));
    }

    Ok(buffer)
}

fn request_commit_plan(
    config: &ResolvedConfig,
    prompt: &llm::Prompt,
    format: OutputFormat,
) -> Result<CommitPlan, ExitCode> {
    request_commit_plan_impl(config, prompt).map_err(|err| map_llm_error(format, err))
}

#[cfg(not(test))]
fn request_commit_plan_impl(
    config: &ResolvedConfig,
    prompt: &llm::Prompt,
) -> Result<CommitPlan, LlmError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| LlmError::Runtime(err.to_string()))?;
    runtime.block_on(llm::generate_commit_plan(config, prompt))
}

#[cfg(test)]
fn request_commit_plan_impl(
    _config: &ResolvedConfig,
    _prompt: &llm::Prompt,
) -> Result<CommitPlan, LlmError> {
    Ok(CommitPlan {
        schema_version: SCHEMA_VERSION.to_string(),
        request_id: None,
        warnings: None,
        input: None,
        plan: vec![atomc_core::types::CommitUnit {
            id: "commit-1".to_string(),
            type_: atomc_core::types::CommitType::Docs,
            scope: Some("cli".to_string()),
            summary: "document CLI plan output and diff input handling examples".to_string(),
            body: vec![
                "Add usage examples for plan output".to_string(),
                "Clarify diff input handling details".to_string(),
            ],
            files: vec!["docs/02_cli_spec.md".to_string()],
            hunks: Vec::new(),
        }],
    })
}

fn execute_apply_plan(request: git::ApplyRequest<'_>) -> Result<Vec<ApplyResult>, GitError> {
    execute_apply_plan_impl(request)
}

#[cfg(not(test))]
fn execute_apply_plan_impl(request: git::ApplyRequest<'_>) -> Result<Vec<ApplyResult>, GitError> {
    git::apply_plan(request)
}

#[cfg(test)]
fn execute_apply_plan_impl(request: git::ApplyRequest<'_>) -> Result<Vec<ApplyResult>, GitError> {
    if APPLY_SHOULD_FAIL.swap(false, std::sync::atomic::Ordering::SeqCst) {
        return Err(GitError::CommandFailed {
            cmd: "git apply (test)".to_string(),
            stderr: "simulated failure".to_string(),
        });
    }

    Ok(applied_results(request.plan))
}

fn map_llm_error(format: OutputFormat, error: LlmError) -> ExitCode {
    match error {
        LlmError::Runtime(message) => emit_error(
            format,
            ErrorCode::LlmRuntimeError,
            "llm request failed",
            Some(serde_json::json!({ "error": message })),
        ),
        LlmError::Parse(message) => emit_error(
            format,
            ErrorCode::LlmParseError,
            "llm response parse failed",
            Some(serde_json::json!({ "error": message })),
        ),
        LlmError::Timeout => emit_error(
            format,
            ErrorCode::Timeout,
            "llm request timed out",
            None,
        ),
        LlmError::UnsupportedRuntime(runtime) => emit_error(
            format,
            ErrorCode::ConfigError,
            "unsupported llm runtime",
            Some(serde_json::json!({ "runtime": runtime })),
        ),
    }
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
    LlmRuntimeError,
    LlmParseError,
    Timeout,
    GitError,
}

impl ErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UsageError => "usage_error",
            ErrorCode::InputInvalid => "input_invalid",
            ErrorCode::ConfigError => "config_error",
            ErrorCode::LlmRuntimeError => "llm_runtime_error",
            ErrorCode::LlmParseError => "llm_parse_error",
            ErrorCode::Timeout => "timeout",
            ErrorCode::GitError => "git_error",
        }
    }

    fn exit_code(self) -> ExitCode {
        match self {
            ErrorCode::UsageError => ExitCode::from(2),
            ErrorCode::InputInvalid => ExitCode::from(3),
            ErrorCode::ConfigError => ExitCode::from(7),
            ErrorCode::LlmRuntimeError => ExitCode::from(4),
            ErrorCode::LlmParseError => ExitCode::from(5),
            ErrorCode::Timeout => ExitCode::from(4),
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

fn emit_plan(format: OutputFormat, plan: &CommitPlan) -> Result<(), ExitCode> {
    match format {
        OutputFormat::Json => {
            let payload = serde_json::to_string(plan).unwrap_or_else(|_| {
                format!(
                    "{{\"schema_version\":\"{}\",\"error\":\"failed to serialize plan\"}}",
                    SCHEMA_VERSION
                )
            });
            println!("{payload}");
            Ok(())
        }
        OutputFormat::Human => {
            print_plan_human(plan);
            Ok(())
        }
    }
}

fn emit_apply(format: OutputFormat, response: &CommitApplyResponse) -> Result<(), ExitCode> {
    match format {
        OutputFormat::Json => {
            let payload = serde_json::to_string(response).unwrap_or_else(|_| {
                format!(
                    "{{\"schema_version\":\"{}\",\"error\":\"failed to serialize apply response\"}}",
                    SCHEMA_VERSION
                )
            });
            println!("{payload}");
            Ok(())
        }
        OutputFormat::Human => {
            print_apply_human(response);
            Ok(())
        }
    }
}

fn print_plan_human(plan: &CommitPlan) {
    println!("Commit plan ({} commits):", plan.plan.len());
    for (idx, unit) in plan.plan.iter().enumerate() {
        let header = match unit.scope.as_deref() {
            Some(scope) => format!("{}[{}]: {}", commit_type_str(&unit.type_), scope, unit.summary),
            None => format!("{}: {}", commit_type_str(&unit.type_), unit.summary),
        };
        println!("{}. {}", idx + 1, header);
        for line in &unit.body {
            println!("   {}", line);
        }
        if !unit.files.is_empty() {
            println!("   files: {}", unit.files.join(", "));
        }
    }
}

fn print_apply_human(response: &CommitApplyResponse) {
    println!("Apply plan ({} commits):", response.plan.len());
    for (idx, unit) in response.plan.iter().enumerate() {
        let header = match unit.scope.as_deref() {
            Some(scope) => format!("{}[{}]: {}", commit_type_str(&unit.type_), scope, unit.summary),
            None => format!("{}: {}", commit_type_str(&unit.type_), unit.summary),
        };
        println!("{}. {}", idx + 1, header);
        for line in &unit.body {
            println!("   {}", line);
        }
        if !unit.files.is_empty() {
            println!("   files: {}", unit.files.join(", "));
        }
        if let Some(result) = response.results.iter().find(|res| res.id == unit.id) {
            println!("   status: {}", apply_status_str(&result.status));
        }
    }
}

fn commit_type_str(commit_type: &atomc_core::types::CommitType) -> &'static str {
    match commit_type {
        atomc_core::types::CommitType::Feat => "feat",
        atomc_core::types::CommitType::Fix => "fix",
        atomc_core::types::CommitType::Refactor => "refactor",
        atomc_core::types::CommitType::Style => "style",
        atomc_core::types::CommitType::Docs => "docs",
        atomc_core::types::CommitType::Test => "test",
        atomc_core::types::CommitType::Chore => "chore",
        atomc_core::types::CommitType::Build => "build",
        atomc_core::types::CommitType::Perf => "perf",
        atomc_core::types::CommitType::Ci => "ci",
    }
}

fn apply_status_str(status: &ApplyStatus) -> &'static str {
    match status {
        ApplyStatus::Planned => "planned",
        ApplyStatus::Applied => "applied",
        ApplyStatus::Skipped => "skipped",
        ApplyStatus::Failed => "failed",
    }
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
        GitError::DiffHashMismatch { expected, actual } => {
            serde_json::json!({ "expected": expected, "actual": actual })
        }
        GitError::HunksNotSupported { id } => serde_json::json!({ "id": id }),
        GitError::PlanFileMissing { id, file } => {
            serde_json::json!({ "id": id, "file": file })
        }
        GitError::StagedFilesMismatch { id, expected, actual } => {
            serde_json::json!({ "id": id, "expected": expected, "actual": actual })
        }
        GitError::StagedDiffEmpty { id } => serde_json::json!({ "id": id }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atomc_core::config::ResolvedConfig;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EnvVarGuard {
        key: String,
        previous: Option<std::ffi::OsString>,
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

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn set_apply_failure(value: bool) {
        super::APPLY_SHOULD_FAIL.store(value, Ordering::SeqCst);
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
        let _lock = lock_env();
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
            assert!(result.is_ok());
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handle_apply_computes_repo_diff_when_missing_input() {
        let _lock = lock_env();
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
            assert!(result.is_ok());
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handle_apply_execute_reports_git_error() {
        let _lock = lock_env();
        set_apply_failure(true);
        let dir = temp_dir("repo-exec");
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
                execute: true,
                cleanup_on_error: true,
                timeout: None,
            }),
        };

        if let Commands::Apply(ref args) = cli.command {
            let result = handle_apply(&cli, args);
            assert_eq!(result.unwrap_err(), ExitCode::from(6));
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handle_plan_reports_git_error_when_diff_fails() {
        let _lock = lock_env();
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

        let _env_guard = EnvVarGuard::set("LOCAL_COMMIT_MAX_DIFF_BYTES", "0");

        if let Commands::Plan(ref args) = cli.command {
            let result = handle_plan(&cli, args);
            assert!(result.is_err());
        }

        fs::remove_dir_all(&dir).ok();
    }
}
