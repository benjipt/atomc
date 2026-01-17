/// Semantic validation for commit plans beyond JSON schema checks.
use crate::types::CommitUnit;

pub type SemanticValidationErrors = Vec<SemanticValidationError>;
pub type SemanticValidationWarnings = Vec<SemanticWarning>;

#[derive(Debug, thiserror::Error)]
pub enum SemanticValidationError {
    #[error("commit {id} has empty id")]
    EmptyId { id: String },
    #[error("commit {id} summary length {len} outside 50-72 chars")]
    SummaryLength { id: String, len: usize },
    #[error("commit {id} has {count} body lines (expected 1-3)")]
    BodyLineCount { id: String, count: usize },
    #[error("commit {id} body line {index} is empty")]
    BodyLineEmpty { id: String, index: usize },
    #[error("commit {id} scope is empty")]
    ScopeEmpty { id: String },
    #[error("commit {id} scope is missing")]
    ScopeMissing { id: String },
    #[error("commit {id} scope is not kebab-case")]
    ScopeInvalid { id: String },
}

/// How to treat missing commit scopes.
#[derive(Debug, Clone, Copy)]
pub enum ScopePolicy {
    Require,
    Allow,
    Warn,
}

/// Non-fatal validation warnings emitted during semantic checks.
#[derive(Debug, Clone)]
pub enum SemanticWarning {
    ScopeMissing { id: String },
}

/// Summary of semantic validation warnings.
#[derive(Debug, Clone, Default)]
pub struct SemanticValidationReport {
    pub warnings: SemanticValidationWarnings,
}

/// Validate commit units and return any non-fatal warnings.
pub fn validate_commit_units(
    units: &[CommitUnit],
    scope_policy: ScopePolicy,
) -> Result<SemanticValidationReport, SemanticValidationErrors> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for unit in units {
        validate_commit_unit(unit, scope_policy, &mut errors, &mut warnings);
    }

    if errors.is_empty() {
        Ok(SemanticValidationReport { warnings })
    } else {
        Err(errors)
    }
}

fn validate_commit_unit(
    unit: &CommitUnit,
    scope_policy: ScopePolicy,
    errors: &mut SemanticValidationErrors,
    warnings: &mut SemanticValidationWarnings,
) {
    let id = unit.id.clone();
    if unit.id.trim().is_empty() {
        errors.push(SemanticValidationError::EmptyId {
            id: id.clone(),
        });
    }

    let summary_len = unit.summary.chars().count();
    if summary_len < 50 || summary_len > 72 {
        errors.push(SemanticValidationError::SummaryLength {
            id: id.clone(),
            len: summary_len,
        });
    }

    let body_len = unit.body.len();
    if body_len < 1 || body_len > 3 {
        errors.push(SemanticValidationError::BodyLineCount {
            id: id.clone(),
            count: body_len,
        });
    }

    for (idx, line) in unit.body.iter().enumerate() {
        if line.trim().is_empty() {
            errors.push(SemanticValidationError::BodyLineEmpty {
                id: id.clone(),
                index: idx,
            });
        }
    }

    match unit.scope.as_deref() {
        Some(scope) if scope.trim().is_empty() => {
            errors.push(SemanticValidationError::ScopeEmpty {
                id: id.clone(),
            });
        }
        Some(scope) if !is_kebab_case(scope) => {
            errors.push(SemanticValidationError::ScopeInvalid {
                id: id.clone(),
            });
        }
        None => match scope_policy {
            ScopePolicy::Require => errors.push(SemanticValidationError::ScopeMissing {
                id: id.clone(),
            }),
            ScopePolicy::Warn => warnings.push(SemanticWarning::ScopeMissing {
                id: id.clone(),
            }),
            ScopePolicy::Allow => {}
        },
        _ => {}
    }
}

fn is_kebab_case(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') || value.ends_with('-') {
        return false;
    }

    value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}
