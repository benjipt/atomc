use atomc_core::schema::{validate_schema, SchemaKind};
use serde_json::json;

fn base_commit_unit() -> serde_json::Value {
    json!({
        "id": "commit-1",
        "type": "feat",
        "scope": "cli",
        "summary": "add deterministic JSON output for plan command results",
        "body": ["Emit JSON by default for automation"],
        "files": ["src/main.rs"],
        "hunks": []
    })
}

#[test]
fn commit_plan_schema_accepts_valid_payload() {
    let payload = json!({
        "schema_version": "v1",
        "plan": [base_commit_unit()]
    });

    let result = validate_schema(SchemaKind::CommitPlan, &payload);
    assert!(result.is_ok());
}

#[test]
fn commit_plan_schema_rejects_short_summary() {
    let mut unit = base_commit_unit();
    unit["summary"] = json!("too short");

    let payload = json!({
        "schema_version": "v1",
        "plan": [unit]
    });

    let result = validate_schema(SchemaKind::CommitPlan, &payload);
    assert!(result.is_err());
}

#[test]
fn commit_apply_schema_accepts_valid_payload() {
    let payload = json!({
        "schema_version": "v1",
        "plan": [base_commit_unit()],
        "results": [
            {"id": "commit-1", "status": "planned", "error": null}
        ]
    });

    let result = validate_schema(SchemaKind::CommitApply, &payload);
    assert!(result.is_ok());
}

#[test]
fn commit_apply_schema_rejects_missing_results() {
    let payload = json!({
        "schema_version": "v1",
        "plan": [base_commit_unit()]
    });

    let result = validate_schema(SchemaKind::CommitApply, &payload);
    assert!(result.is_err());
}

#[test]
fn error_schema_accepts_valid_payload() {
    let payload = json!({
        "schema_version": "v1",
        "error": {
            "code": "input_invalid",
            "message": "stdin is empty",
            "details": {"hint": "provide a diff"}
        }
    });

    let result = validate_schema(SchemaKind::ErrorResponse, &payload);
    assert!(result.is_ok());
}

#[test]
fn error_schema_rejects_unknown_code() {
    let payload = json!({
        "schema_version": "v1",
        "error": {
            "code": "unknown_error",
            "message": "bad"
        }
    });

    let result = validate_schema(SchemaKind::ErrorResponse, &payload);
    assert!(result.is_err());
}
