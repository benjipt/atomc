#[path = "support/integration.rs"]
mod integration_support;
mod support;

use serde_json::{json, Value};
use integration_support::{
    init_repo_with_change, reserve_port, run_git, start_atomc_server, wait_for_port,
};
use support::{run_atomc, start_mock_ollama};
use std::fs;

const SUMMARY: &str = "add integration test coverage for cli and apply flows";
const SCOPE: &str = "cli-tests";

fn plan_payload(files: &[&str]) -> String {
    let plan = json!({
        "schema_version": "v1",
        "plan": [{
            "id": "commit-1",
            "type": "test",
            "scope": SCOPE,
            "summary": SUMMARY,
            "body": [
                "Add CLI integration test coverage for core flows",
                "Exercise apply execution path in a temp repo"
            ],
            "files": files,
            "hunks": []
        }]
    });
    plan.to_string()
}

#[tokio::test]
async fn cli_plan_accepts_stdin_diff() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let diff = run_git(repo.path(), &["diff"]);

    let stdout = run_atomc(
        &["plan", "--format", "json"],
        repo.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;
    let value: Value = serde_json::from_str(&stdout).expect("plan json");
    assert_eq!(value["schema_version"], "v1");
    assert_eq!(value["input"]["source"], "diff");
    assert_eq!(value["plan"][0]["files"][0], "file.txt");
}

#[tokio::test]
async fn cli_plan_accepts_diff_file() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let diff = run_git(repo.path(), &["diff"]);
    let diff_path = repo.path().join("plan.diff");
    fs::write(&diff_path, diff).expect("write diff");

    let stdout = run_atomc(
        &[
            "plan",
            "--diff-file",
            diff_path.to_str().expect("diff path"),
            "--format",
            "json",
        ],
        repo.path(),
        &mock.base_url,
        None,
    )
    .await;
    let value: Value = serde_json::from_str(&stdout).expect("plan json");
    assert_eq!(value["input"]["source"], "diff");
    assert_eq!(value["plan"][0]["files"][0], "file.txt");
}

#[tokio::test]
async fn cli_plan_human_format_emits_text() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let diff = run_git(repo.path(), &["diff"]);

    let stdout = run_atomc(
        &["plan", "--format", "human"],
        repo.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;
    assert!(stdout.contains("Commit plan (1 commits):"));
    assert!(stdout.contains(SUMMARY));
    assert!(!stdout.trim_start().starts_with('{'));
}

#[tokio::test]
async fn cli_apply_dry_run_accepts_stdin_diff() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let diff = run_git(repo.path(), &["diff"]);

    let stdout = run_atomc(
        &["apply", "--repo", repo.path().to_str().expect("repo path"), "--format", "json"],
        repo.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;
    let value: Value = serde_json::from_str(&stdout).expect("apply json");
    assert_eq!(value["schema_version"], "v1");
    assert_eq!(value["input"]["source"], "diff");
    assert_eq!(value["results"][0]["status"], "planned");
}

#[tokio::test]
async fn cli_apply_human_format_emits_text() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let diff = run_git(repo.path(), &["diff"]);

    let stdout = run_atomc(
        &[
            "apply",
            "--repo",
            repo.path().to_str().expect("repo path"),
            "--format",
            "human",
        ],
        repo.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;
    assert!(stdout.contains("Apply plan (1 commits):"));
    assert!(stdout.contains(SUMMARY));
    assert!(!stdout.trim_start().starts_with('{'));
}

#[tokio::test]
async fn cli_apply_execute_creates_commit() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let diff = run_git(repo.path(), &["diff"]);

    let stdout = run_atomc(
        &[
            "apply",
            "--repo",
            repo.path().to_str().expect("repo path"),
            "--execute",
            "--format",
            "json",
        ],
        repo.path(),
        &mock.base_url,
        Some(&diff),
    )
    .await;
    let value: Value = serde_json::from_str(&stdout).expect("apply json");
    assert_eq!(value["results"][0]["status"], "applied");
    let subject = run_git(repo.path(), &["log", "-1", "--pretty=%s"]);
    assert_eq!(subject.trim(), format!("test[{SCOPE}]: {SUMMARY}"));
}

#[tokio::test]
async fn http_plan_with_repo_diff() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let port = reserve_port();
    let server = start_atomc_server(port, &mock.base_url);
    wait_for_port(port).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/commit-plan", server.base_url))
        .json(&json!({ "repo_path": repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("plan response");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("plan json");
    assert_eq!(payload["input"]["source"], "repo");
    assert_eq!(payload["plan"][0]["files"][0], "file.txt");
}

#[tokio::test]
async fn http_apply_dry_run_with_repo_diff() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let port = reserve_port();
    let server = start_atomc_server(port, &mock.base_url);
    wait_for_port(port).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/commit-apply", server.base_url))
        .json(&json!({ "repo_path": repo.path().to_string_lossy(), "execute": false }))
        .send()
        .await
        .expect("apply response");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("apply json");
    assert_eq!(payload["results"][0]["status"], "planned");
}

#[tokio::test]
async fn http_apply_dry_run_with_explicit_diff() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json).await;
    let diff = run_git(repo.path(), &["diff"]);
    let port = reserve_port();
    let server = start_atomc_server(port, &mock.base_url);
    wait_for_port(port).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/commit-apply", server.base_url))
        .json(&json!({
            "repo_path": repo.path().to_string_lossy(),
            "diff": diff,
            "execute": false
        }))
        .send()
        .await
        .expect("apply response");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("apply json");
    assert_eq!(payload["input"]["source"], "diff");
    assert_eq!(payload["results"][0]["status"], "planned");
}

#[tokio::test]
async fn http_apply_accepts_explicit_plan() {
    let repo = init_repo_with_change();
    let plan_json = plan_payload(&["file.txt"]);
    let mock = start_mock_ollama(plan_json.clone()).await;
    let plan_value: Value = serde_json::from_str(&plan_json).expect("plan json");
    let plan = plan_value["plan"].clone();
    let port = reserve_port();
    let server = start_atomc_server(port, &mock.base_url);
    wait_for_port(port).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/commit-apply", server.base_url))
        .json(&json!({
            "repo_path": repo.path().to_string_lossy(),
            "plan": plan,
            "execute": false
        }))
        .send()
        .await
        .expect("apply response");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("apply json");
    assert_eq!(payload["results"][0]["status"], "planned");
    assert_eq!(payload["plan"][0]["files"][0], "file.txt");
}
