use atomc_core::semantic::{validate_commit_units, SemanticValidationError};
use atomc_core::types::{CommitType, CommitUnit, Hunk};

fn base_unit() -> CommitUnit {
    CommitUnit {
        id: "commit-1".to_string(),
        type_: CommitType::Feat,
        scope: Some("cli".to_string()),
        summary: "add deterministic JSON output for plan command results".to_string(),
        body: vec!["Emit JSON by default for automation".to_string()],
        files: vec!["src/main.rs".to_string()],
        hunks: Vec::new(),
    }
}

#[test]
fn valid_commit_unit_passes_validation() {
    let unit = base_unit();
    let result = validate_commit_units(&[unit]);
    assert!(result.is_ok());
}

#[test]
fn invalid_summary_length_is_reported() {
    let mut unit = base_unit();
    unit.summary = "too short".to_string();

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::SummaryLength { .. })));
}

#[test]
fn invalid_body_count_is_reported() {
    let mut unit = base_unit();
    unit.body = vec!["one".to_string(), "two".to_string(), "three".to_string(), "four".to_string()];

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::BodyLineCount { .. })));
}

#[test]
fn empty_body_line_is_reported() {
    let mut unit = base_unit();
    unit.body = vec!["".to_string()];

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::BodyLineEmpty { .. })));
}

#[test]
fn empty_scope_is_reported() {
    let mut unit = base_unit();
    unit.scope = Some(" ".to_string());

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::ScopeEmpty { .. })));
}

#[test]
fn invalid_scope_format_is_reported() {
    let mut unit = base_unit();
    unit.scope = Some("Bad_Scope".to_string());

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::ScopeInvalid { .. })));
}

#[test]
fn empty_id_is_reported() {
    let mut unit = base_unit();
    unit.id = "".to_string();

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::EmptyId { .. })));
}

#[test]
fn scope_none_is_allowed_for_global_changes() {
    let mut unit = base_unit();
    unit.scope = None;

    let result = validate_commit_units(&[unit]);
    assert!(result.is_ok());
}

#[test]
fn kebab_case_scope_is_allowed() {
    let mut unit = base_unit();
    unit.scope = Some("cli-tools".to_string());

    let result = validate_commit_units(&[unit]);
    assert!(result.is_ok());
}

#[test]
fn scope_with_trailing_dash_is_rejected() {
    let mut unit = base_unit();
    unit.scope = Some("cli-".to_string());

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::ScopeInvalid { .. })));
}

#[test]
fn scope_with_leading_dash_is_rejected() {
    let mut unit = base_unit();
    unit.scope = Some("-cli".to_string());

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.iter().any(|err| matches!(err, SemanticValidationError::ScopeInvalid { .. })));
}

#[test]
fn multiple_errors_are_accumulated() {
    let mut unit = base_unit();
    unit.id = "".to_string();
    unit.summary = "short".to_string();
    unit.body = Vec::new();
    unit.scope = Some("Bad_Scope".to_string());
    unit.hunks = vec![Hunk {
        file: "src/main.rs".to_string(),
        header: "@@ -1 +1 @@".to_string(),
        id: None,
    }];

    let errors = validate_commit_units(&[unit]).unwrap_err();
    assert!(errors.len() >= 3);
}
