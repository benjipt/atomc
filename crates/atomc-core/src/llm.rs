use crate::config::{DiffMode, ResolvedConfig, Runtime};
use crate::schema::{self, SchemaKind};
use crate::types::CommitPlan;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("llm runtime error: {0}")]
    Runtime(String),
    #[error("llm output parse error: {0}")]
    Parse(String),
    #[error("llm request timed out")]
    Timeout,
    #[error("unsupported runtime: {0}")]
    UnsupportedRuntime(String),
}

#[derive(Debug, Clone)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

#[derive(Debug, Clone)]
pub struct PromptContext<'a> {
    pub repo_path: Option<&'a Path>,
    pub diff_mode: Option<DiffMode>,
    pub include_untracked: Option<bool>,
    pub git_status: Option<&'a str>,
    pub diff: &'a str,
}

#[derive(Debug, Clone)]
pub struct LlmOptions {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout: Duration,
}

impl LlmOptions {
    pub fn from_config(config: &ResolvedConfig) -> Self {
        Self {
            model: config.model.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            timeout: Duration::from_secs(config.llm_timeout_secs),
        }
    }
}

pub fn build_prompt(context: PromptContext<'_>) -> Prompt {
    Prompt {
        system: SYSTEM_PROMPT.to_string(),
        user: build_user_prompt(context),
    }
}

fn build_user_prompt(context: PromptContext<'_>) -> String {
    let repo_path = context
        .repo_path
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let diff_mode = context
        .diff_mode
        .map(|mode| match mode {
            DiffMode::Worktree => "worktree",
            DiffMode::Staged => "staged",
            DiffMode::All => "all",
        })
        .unwrap_or_default();
    let include_untracked = context
        .include_untracked
        .map(|value| value.to_string())
        .unwrap_or_default();
    let git_status = context.git_status.unwrap_or_default();

    format!(
        "You will be given a git diff and optional repo metadata.\n\
Produce an atomic commit plan as JSON only.\n\n\
Context:\n\
- repo_path: {repo_path}\n\
- diff_mode: {diff_mode}\n\
- include_untracked: {include_untracked}\n\
- git_status: {git_status}\n\n\
Diff:\n\
{diff}",
        diff = context.diff
    )
}

pub struct OllamaClient {
    base_url: String,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: HTTP_CLIENT.clone(),
        }
    }

    pub async fn generate_commit_plan(
        &self,
        prompt: &Prompt,
        options: &LlmOptions,
    ) -> Result<CommitPlan, LlmError> {
        let request = OllamaGenerateRequest {
            model: &options.model,
            prompt: &prompt.user,
            system: &prompt.system,
            stream: false,
            options: OllamaOptions {
                temperature: options.temperature,
                num_predict: options.max_tokens,
            },
        };
        let url = format!(
            "{}/api/generate",
            self.base_url.trim_end_matches('/')
        );

        let response = self
            .http
            .post(url)
            .json(&request)
            .timeout(options.timeout)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|err| LlmError::Runtime(format!("status {status}: {err}")))?;
            return Err(LlmError::Runtime(format!(
                "status {status}: {body}"
            )));
        }

        let payload: OllamaGenerateResponse = response
            .json()
            .await
            .map_err(|err| LlmError::Parse(err.to_string()))?;
        if let Some(error) = payload.error {
            return Err(LlmError::Runtime(error));
        }

        let response_text = payload
            .response
            .ok_or_else(|| LlmError::Parse("missing response".to_string()))?;
        parse_commit_plan(&response_text)
    }
}

pub struct LlamaCppClient {
    base_url: String,
    http: reqwest::Client,
}

impl LlamaCppClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: HTTP_CLIENT.clone(),
        }
    }

    pub async fn generate_commit_plan(
        &self,
        prompt: &Prompt,
        options: &LlmOptions,
    ) -> Result<CommitPlan, LlmError> {
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let request = LlamaCppChatRequest {
            model: &options.model,
            messages: vec![
                LlamaCppMessage {
                    role: "system",
                    content: &prompt.system,
                },
                LlamaCppMessage {
                    role: "user",
                    content: &prompt.user,
                },
            ],
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            stream: false,
        };

        let response = self
            .http
            .post(url)
            .json(&request)
            .timeout(options.timeout)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| LlmError::Runtime(format!("status {status}: {err}")))?;
        if !status.is_success() {
            return Err(LlmError::Runtime(format!("status {status}: {body}")));
        }

        let value: Value =
            serde_json::from_str(&body).map_err(|err| LlmError::Parse(err.to_string()))?;
        if let Some(error) = llama_cpp_error_message(&value) {
            return Err(LlmError::Runtime(error));
        }
        let content = value
            .pointer("/choices/0/message/content")
            .and_then(|value| value.as_str())
            .or_else(|| value.pointer("/choices/0/text").and_then(|value| value.as_str()))
            .ok_or_else(|| LlmError::Parse("missing chat completion content".to_string()))?;

        parse_commit_plan(content)
    }
}

pub async fn generate_commit_plan(
    config: &ResolvedConfig,
    prompt: &Prompt,
) -> Result<CommitPlan, LlmError> {
    let options = LlmOptions::from_config(config);
    match config.runtime {
        Runtime::Ollama => {
            let client = OllamaClient::new(config.ollama_url.clone());
            client.generate_commit_plan(prompt, &options).await
        }
        Runtime::LlamaCpp => {
            let client = LlamaCppClient::new(config.ollama_url.clone());
            client.generate_commit_plan(prompt, &options).await
        }
    }
}

fn parse_commit_plan(payload: &str) -> Result<CommitPlan, LlmError> {
    let value: Value = serde_json::from_str(payload.trim())
        .map_err(|err| LlmError::Parse(err.to_string()))?;
    schema::validate_schema(SchemaKind::CommitPlan, &value)
        .map_err(|err| LlmError::Parse(err.to_string()))?;
    let plan: CommitPlan = serde_json::from_value(value)
        .map_err(|err| LlmError::Parse(err.to_string()))?;
    if plan.plan.is_empty() {
        return Err(LlmError::Parse("plan is empty".to_string()));
    }
    Ok(plan)
}

fn map_reqwest_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Runtime(error.to_string())
    }
}

fn llama_cpp_error_message(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    if error.is_null() {
        return None;
    }
    if let Some(message) = error.get("message").and_then(|value| value.as_str()) {
        return Some(message.to_string());
    }
    if let Some(message) = error.as_str() {
        return Some(message.to_string());
    }
    Some(error.to_string())
}

#[derive(Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: u32,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct LlamaCppChatRequest<'a> {
    model: &'a str,
    messages: Vec<LlamaCppMessage<'a>>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Serialize)]
struct LlamaCppMessage<'a> {
    role: &'a str,
    content: &'a str,
}

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

const SYSTEM_PROMPT: &str = "You are a local commit planning assistant.\n\
Return a single JSON object that matches the CommitPlan schema.\n\
Do not include Markdown, comments, or any extra text.\n\
Follow atomic commit rules:\n\
- Each commit must do exactly one thing.\n\
- Split unrelated concerns into separate commits.\n\
- Foundations first, integrations last.\n\
- Avoid bundling refactors with feature changes.\n\
Commit message rules:\n\
- Use conventional commits: type[scope]: summary\n\
- Scope is required unless the change is truly global.\n\
- Summary is imperative, 50-72 chars.\n\
- Body is 1-3 short lines (no leading hyphens).\n\
If any required field is unknown, infer the best value.";
