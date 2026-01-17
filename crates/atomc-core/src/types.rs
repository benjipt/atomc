/// Schema-aligned types used by CLI and server JSON responses.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitPlan {
    pub schema_version: String,
    pub request_id: Option<String>,
    pub warnings: Option<Vec<Warning>>,
    pub input: Option<InputMeta>,
    pub plan: Vec<CommitUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitApplyResponse {
    pub schema_version: String,
    pub request_id: Option<String>,
    pub warnings: Option<Vec<Warning>>,
    pub input: Option<InputMeta>,
    pub plan: Vec<CommitUnit>,
    pub results: Vec<ApplyResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub schema_version: String,
    pub request_id: Option<String>,
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMeta {
    pub source: InputSource,
    pub diff_mode: Option<DiffMode>,
    pub include_untracked: Option<bool>,
    pub diff_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    Repo,
    Diff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode {
    Worktree,
    Staged,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitUnit {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: CommitType,
    pub scope: Option<String>,
    pub summary: String,
    pub body: Vec<String>,
    pub files: Vec<String>,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitType {
    Feat,
    Fix,
    Refactor,
    Style,
    Docs,
    Test,
    Chore,
    Build,
    Perf,
    Ci,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub file: String,
    pub header: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub id: String,
    pub status: ApplyStatus,
    pub commit_hash: Option<String>,
    pub error: Option<ErrorDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    Planned,
    Applied,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}
