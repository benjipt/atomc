use jsonschema::{Draft, JSONSchema};
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub enum SchemaKind {
    CommitPlan,
    CommitApply,
    ErrorResponse,
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaValidationError {
    #[error("schema JSON parse error: {0}")]
    SchemaParse(#[from] serde_json::Error),
    #[error("schema compile error: {0}")]
    SchemaCompile(String),
    #[error("schema validation errors: {0:?}")]
    SchemaViolation(Vec<String>),
}

pub fn validate_schema(kind: SchemaKind, instance: &Value) -> Result<(), SchemaValidationError> {
    let schema = schema_for(kind)?;
    schema
        .validate(instance)
        .map_err(|errors| SchemaValidationError::SchemaViolation(errors.map(|e| e.to_string()).collect()))?;
    Ok(())
}

fn schema_for(kind: SchemaKind) -> Result<&'static JSONSchema, SchemaValidationError> {
    match kind {
        SchemaKind::CommitPlan => COMMIT_PLAN_SCHEMA.get_or_try_init(|| compile_schema(COMMIT_PLAN_SCHEMA_STR)),
        SchemaKind::CommitApply => COMMIT_APPLY_SCHEMA.get_or_try_init(|| compile_schema(COMMIT_APPLY_SCHEMA_STR)),
        SchemaKind::ErrorResponse => ERROR_SCHEMA.get_or_try_init(|| compile_schema(ERROR_SCHEMA_STR)),
    }
}

fn compile_schema(schema_str: &str) -> Result<JSONSchema, SchemaValidationError> {
    let schema_value: Value = serde_json::from_str(schema_str)?;
    JSONSchema::options()
        .with_draft(Draft::Draft202012)
        .compile(&schema_value)
        .map_err(|err| SchemaValidationError::SchemaCompile(err.to_string()))
}

static COMMIT_PLAN_SCHEMA: OnceLock<JSONSchema> = OnceLock::new();
static COMMIT_APPLY_SCHEMA: OnceLock<JSONSchema> = OnceLock::new();
static ERROR_SCHEMA: OnceLock<JSONSchema> = OnceLock::new();

const COMMIT_PLAN_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/commit-plan.json"));
const COMMIT_APPLY_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/commit-apply.json"));
const ERROR_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/error.json"));
