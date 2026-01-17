/// JSON Schema validation helpers for CLI/server responses.
///
/// Validators are cached to avoid recompiling the same schema on each call.
use jsonschema::Validator;
use once_cell::sync::Lazy;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub enum SchemaKind {
    CommitPlan,
    CommitApply,
    ErrorResponse,
}

/// Validation errors for schema compilation and instance checks.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SchemaValidationError {
    #[error("schema JSON parse error: {0}")]
    SchemaParse(String),
    #[error("schema compile error: {0}")]
    SchemaCompile(String),
    #[error("schema validation errors: {0:?}")]
    SchemaViolation(Vec<String>),
}

/// Validate a JSON value against the cached schema for the given kind.
pub fn validate_schema(kind: SchemaKind, instance: &Value) -> Result<(), SchemaValidationError> {
    let schema = schema_for(kind)?;
    let errors: Vec<String> = schema.iter_errors(instance).map(|e| e.to_string()).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(SchemaValidationError::SchemaViolation(errors))
    }
}

/// Return the cached validator for the schema kind.
fn schema_for(kind: SchemaKind) -> Result<&'static Validator, SchemaValidationError> {
    match kind {
        SchemaKind::CommitPlan => COMMIT_PLAN_SCHEMA.as_ref(),
        SchemaKind::CommitApply => COMMIT_APPLY_SCHEMA.as_ref(),
        SchemaKind::ErrorResponse => ERROR_SCHEMA.as_ref(),
    }
    .map_err(|err| err.clone())
}

/// Compile a schema document into a Draft 2020-12 validator.
fn compile_schema(schema_str: &str) -> Result<Validator, SchemaValidationError> {
    let schema_value: Value = serde_json::from_str(schema_str)
        .map_err(|err| SchemaValidationError::SchemaParse(err.to_string()))?;
    jsonschema::draft202012::options()
        .build(&schema_value)
        .map_err(|err| SchemaValidationError::SchemaCompile(err.to_string()))
}

static COMMIT_PLAN_SCHEMA: Lazy<Result<Validator, SchemaValidationError>> =
    Lazy::new(|| compile_schema(COMMIT_PLAN_SCHEMA_STR));
static COMMIT_APPLY_SCHEMA: Lazy<Result<Validator, SchemaValidationError>> =
    Lazy::new(|| compile_schema(COMMIT_APPLY_SCHEMA_STR));
static ERROR_SCHEMA: Lazy<Result<Validator, SchemaValidationError>> =
    Lazy::new(|| compile_schema(ERROR_SCHEMA_STR));

const COMMIT_PLAN_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/commit-plan.json"));
const COMMIT_APPLY_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/commit-apply.json"));
const ERROR_SCHEMA_STR: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/v1/error.json"));
