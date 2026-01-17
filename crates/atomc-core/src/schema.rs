use jsonschema::Validator;
use serde_json::Value;

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
    let errors: Vec<String> = schema.iter_errors(instance).map(|e| e.to_string()).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(SchemaValidationError::SchemaViolation(errors))
    }
}

fn schema_for(kind: SchemaKind) -> Result<Validator, SchemaValidationError> {
    match kind {
        SchemaKind::CommitPlan => compile_schema(COMMIT_PLAN_SCHEMA_STR),
        SchemaKind::CommitApply => compile_schema(COMMIT_APPLY_SCHEMA_STR),
        SchemaKind::ErrorResponse => compile_schema(ERROR_SCHEMA_STR),
    }
}

fn compile_schema(schema_str: &str) -> Result<Validator, SchemaValidationError> {
    let schema_value: Value = serde_json::from_str(schema_str)?;
    jsonschema::draft202012::options()
        .build(&schema_value)
        .map_err(|err| SchemaValidationError::SchemaCompile(err.to_string()))
}

const COMMIT_PLAN_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/commit-plan.json"));
const COMMIT_APPLY_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/commit-apply.json"));
const ERROR_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/error.json"));
