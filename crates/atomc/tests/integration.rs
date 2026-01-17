use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

const SUMMARY: &str = "add integration test coverage for cli and apply flows";
const SCOPE: &str = "cli-tests";

struct MockOllama {
    base_url: String,
    handle: JoinHandle<()>,
}

impl Drop for MockOllama {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

struct AtomcServer {
    base_url: String,
    child: Child,
}

impl Drop for AtomcServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn atomc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atomc"))
}

fn init_repo_with_change() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    run_git(dir.path(), &["init"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Atomc Test"]);
    std::fs::write(dir.path().join("file.txt"), "line-1\n").expect("write file");
    run_git(dir.path(), &["add", "file.txt"]);
    run_git(dir.path(), &["commit", "-m", "init"]);
    std::fs::write(dir.path().join("file.txt"), "line-1\nline-2\n").expect("write change");
    dir
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

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

async fn start_mock_ollama(plan_json: String) -> MockOllama {
    let state = Arc::new(plan_json);
    let app = Router::new()
        .route(
            "/api/generate",
            post(|State(plan): State<Arc<String>>, Json(_): Json<Value>| async move {
                Json(json!({ "response": (*plan).clone() }))
            }),
        )
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    MockOllama {
        base_url: format!("http://{addr}"),
        handle,
    }
}

fn reserve_port() -> u16 {
    StdTcpListener::bind("127.0.0.1:0")
        .expect("reserve port")
        .local_addr()
        .expect("port addr")
        .port()
}

fn start_atomc_server(port: u16, ollama_url: &str) -> AtomcServer {
    let mut cmd = Command::new(atomc_bin());
    cmd.arg("serve")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--request-timeout")
        .arg("5")
        .env("LOCAL_COMMIT_RUNTIME", "ollama")
        .env("LOCAL_COMMIT_OLLAMA_URL", ollama_url)
        .env_remove("LOCAL_COMMIT_AGENT_CONFIG")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().expect("spawn atomc serve");
    AtomcServer {
        base_url: format!("http://127.0.0.1:{port}"),
        child,
    }
}

async fn wait_for_port(port: u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    for _ in 0..40 {
        if TcpStream::connect(addr).await.is_ok() {
            return;
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not start on port {port}");
}

async fn run_atomc(args: &[&str], dir: &Path, ollama_url: &str, input: Option<&str>) -> String {
    let args = args.iter().map(|value| value.to_string()).collect::<Vec<_>>();
    let dir = dir.to_path_buf();
    let ollama_url = ollama_url.to_string();
    let input = input.map(|value| value.to_string());

    tokio::task::spawn_blocking(move || {
        run_atomc_sync(args, dir, ollama_url, input)
    })
    .await
    .expect("spawn blocking")
}

fn run_atomc_sync(
    args: Vec<String>,
    dir: PathBuf,
    ollama_url: String,
    input: Option<String>,
) -> String {
    let mut cmd = Command::new(atomc_bin());
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
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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
