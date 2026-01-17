mod support;

use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use support::{atomc_bin, run_atomc, start_mock_ollama};
use tempfile::TempDir;

struct GoldenCase {
    diff: &'static str,
    plan: &'static str,
}

struct ApplyCase {
    plan: &'static str,
    ok: bool,
}

#[tokio::test]
async fn golden_plan_fixtures_match_cli_output() {
    let cases = [
        GoldenCase {
            diff: "diffs/simple_feature.diff",
            plan: "plans/simple_feature.plan.json",
        },
        GoldenCase {
            diff: "diffs/mixed_concerns.diff",
            plan: "plans/mixed_concerns.plan.json",
        },
        GoldenCase {
            diff: "diffs/refactor_plus_feature.diff",
            plan: "plans/refactor_plus_feature.plan.json",
        },
    ];

    for case in cases {
        let diff = load_fixture(case.diff);
        let expected_json = load_fixture(case.plan);
        let mock = start_mock_ollama(expected_json.clone()).await;
        let cwd = TempDir::new().expect("temp dir");

        let stdout = run_atomc(
            &["plan", "--format", "json"],
            cwd.path(),
            &mock.base_url,
            Some(&diff),
        )
        .await;

        let output: Value = serde_json::from_str(&stdout).expect("plan json");
        let expected: Value = serde_json::from_str(&expected_json).expect("fixture json");

        assert_eq!(output["schema_version"], "v1");
        assert_eq!(output["input"]["source"], "diff");
        assert!(output.get("warnings").map_or(true, |value| value.is_null()));
        assert_eq!(output["plan"], expected["plan"]);
    }
}

#[test]
fn golden_apply_fixtures_validate_schema() {
    let cases = [
        ApplyCase {
            plan: "plans/valid_apply.plan.json",
            ok: true,
        },
        ApplyCase {
            plan: "plans/invalid_apply.plan.json",
            ok: false,
        },
    ];

    for case in cases {
        let payload = load_fixture(case.plan);
        let value: Value = serde_json::from_str(&payload).expect("fixture json");
        let result = atomc_core::schema::validate_schema(
            atomc_core::schema::SchemaKind::CommitApply,
            &value,
        );
        assert_eq!(result.is_ok(), case.ok, "fixture {}", case.plan);
    }
}

#[tokio::test]
async fn golden_plan_rejects_invalid_json() {
    let diff = load_fixture("diffs/simple_feature.diff");
    let invalid = load_fixture("plans/invalid_json.txt");
    let mock = start_mock_ollama(invalid).await;
    let cwd = TempDir::new().expect("temp dir");

    let stdout = run_atomc_expect_failure(
        &["plan", "--format", "json"],
        cwd.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;

    let output: Value = serde_json::from_str(&stdout).expect("error json");
    assert_eq!(output["error"]["code"], "llm_parse_error");
}

#[tokio::test]
async fn golden_plan_rejects_schema_violation() {
    let diff = load_fixture("diffs/simple_feature.diff");
    let invalid = load_fixture("plans/invalid_schema.plan.json");
    let mock = start_mock_ollama(invalid).await;
    let cwd = TempDir::new().expect("temp dir");

    let stdout = run_atomc_expect_failure(
        &["plan", "--format", "json"],
        cwd.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;

    let output: Value = serde_json::from_str(&stdout).expect("error json");
    assert_eq!(output["error"]["code"], "llm_parse_error");
}

#[tokio::test]
async fn golden_plan_rejects_semantic_violation() {
    let diff = load_fixture("diffs/simple_feature.diff");
    let invalid = load_fixture("plans/invalid_semantic.plan.json");
    let mock = start_mock_ollama(invalid).await;
    let cwd = TempDir::new().expect("temp dir");

    let stdout = run_atomc_expect_failure(
        &["plan", "--format", "json"],
        cwd.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;

    let output: Value = serde_json::from_str(&stdout).expect("error json");
    assert_eq!(output["error"]["code"], "llm_parse_error");
}

fn load_fixture(relative: &str) -> String {
    let path = fixtures_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {}: {}", path.display(), err))
}

async fn run_atomc_expect_failure(
    args: &[&str],
    dir: &Path,
    ollama_url: &str,
    input: Option<&str>,
) -> String {
    let args = args.iter().map(|value| value.to_string()).collect::<Vec<_>>();
    let dir = dir.to_path_buf();
    let ollama_url = ollama_url.to_string();
    let input = input.map(|value| value.to_string());

    tokio::task::spawn_blocking(move || run_atomc_expect_failure_sync(args, dir, ollama_url, input))
        .await
        .expect("spawn blocking")
}

fn run_atomc_expect_failure_sync(
    args: Vec<String>,
    dir: std::path::PathBuf,
    ollama_url: String,
    input: Option<String>,
) -> String {
    let mut cmd = std::process::Command::new(atomc_bin());
    cmd.args(args)
        .current_dir(dir)
        .env("LOCAL_COMMIT_RUNTIME", "ollama")
        .env("LOCAL_COMMIT_OLLAMA_URL", ollama_url)
        .env("LOCAL_COMMIT_LLM_TIMEOUT_SECS", "5")
        .env_remove("LOCAL_COMMIT_AGENT_CONFIG")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().expect("spawn atomc");
    if let Some(payload) = input {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(payload.as_bytes())
            .expect("write stdin");
    }
    let output = child.wait_with_output().expect("atomc output");
    assert!(
        !output.status.success(),
        "atomc unexpectedly succeeded"
    );
    if !output.stdout.is_empty() {
        return String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

#[tokio::test]
async fn log_diff_enabled_emits_preview() {
    let diff = load_fixture("diffs/simple_feature.diff");
    let expected_json = load_fixture("plans/simple_feature.plan.json");
    let mock = start_mock_ollama(expected_json).await;
    let cwd = TempDir::new().expect("temp dir");

    let stderr = run_atomc_stderr(
        &["--log-level", "debug", "plan", "--format", "json", "--log-diff"],
        cwd.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;

    assert!(stderr.contains("diff logging enabled"));
}

#[tokio::test]
async fn log_diff_disabled_does_not_emit_preview() {
    let diff = load_fixture("diffs/simple_feature.diff");
    let expected_json = load_fixture("plans/simple_feature.plan.json");
    let mock = start_mock_ollama(expected_json).await;
    let cwd = TempDir::new().expect("temp dir");

    let stderr = run_atomc_stderr(
        &["--log-level", "debug", "plan", "--format", "json"],
        cwd.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;

    assert!(!stderr.contains("diff logging enabled"));
}

async fn run_atomc_stderr(
    args: &[&str],
    dir: &Path,
    ollama_url: &str,
    input: Option<&str>,
) -> String {
    let args = args.iter().map(|value| value.to_string()).collect::<Vec<_>>();
    let dir = dir.to_path_buf();
    let ollama_url = ollama_url.to_string();
    let input = input.map(|value| value.to_string());

    tokio::task::spawn_blocking(move || run_atomc_stderr_sync(args, dir, ollama_url, input))
        .await
        .expect("spawn blocking")
}

fn run_atomc_stderr_sync(
    args: Vec<String>,
    dir: std::path::PathBuf,
    ollama_url: String,
    input: Option<String>,
) -> String {
    let mut cmd = std::process::Command::new(atomc_bin());
    cmd.args(args)
        .current_dir(dir)
        .env("LOCAL_COMMIT_RUNTIME", "ollama")
        .env("LOCAL_COMMIT_OLLAMA_URL", ollama_url)
        .env("LOCAL_COMMIT_LLM_TIMEOUT_SECS", "5")
        .env_remove("LOCAL_COMMIT_AGENT_CONFIG")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().expect("spawn atomc");
    if let Some(payload) = input {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(payload.as_bytes())
            .expect("write stdin");
    }
    let output = child.wait_with_output().expect("atomc output");
    assert!(
        output.status.success(),
        "atomc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

fn fixtures_root() -> std::path::PathBuf {
    workspace_root().join("tests/fixtures")
}

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}
