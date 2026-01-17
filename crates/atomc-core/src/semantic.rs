use crate::types::CommitUnit;

pub type SemanticValidationErrors = Vec<SemanticValidationError>;

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
    #[error("commit {id} scope is not kebab-case")]
    ScopeInvalid { id: String },
}

pub fn validate_commit_units(units: &[CommitUnit]) -> Result<(), SemanticValidationErrors> {
    let mut errors = Vec::new();
    for unit in units {
        validate_commit_unit(unit, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_commit_unit(unit: &CommitUnit, errors: &mut SemanticValidationErrors) {
    if unit.id.trim().is_empty() {
        errors.push(SemanticValidationError::EmptyId {
            id: unit.id.clone(),
        });
    }

    let summary_len = unit.summary.chars().count();
    if summary_len < 50 || summary_len > 72 {
        errors.push(SemanticValidationError::SummaryLength {
            id: unit.id.clone(),
            len: summary_len,
        });
    }

    let body_len = unit.body.len();
    if body_len < 1 || body_len > 3 {
        errors.push(SemanticValidationError::BodyLineCount {
            id: unit.id.clone(),
            count: body_len,
        });
    }

    for (idx, line) in unit.body.iter().enumerate() {
        if line.trim().is_empty() {
            errors.push(SemanticValidationError::BodyLineEmpty {
                id: unit.id.clone(),
                index: idx,
            });
        }
    }

    if let Some(scope) = unit.scope.as_deref() {
        if scope.trim().is_empty() {
            errors.push(SemanticValidationError::ScopeEmpty {
                id: unit.id.clone(),
            });
        } else if !is_kebab_case(scope) {
            errors.push(SemanticValidationError::ScopeInvalid {
                id: unit.id.clone(),
            });
        }
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
