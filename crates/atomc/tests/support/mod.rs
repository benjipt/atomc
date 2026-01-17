use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub struct MockOllama {
    pub base_url: String,
    handle: JoinHandle<()>,
}

impl Drop for MockOllama {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub fn atomc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atomc"))
}

pub async fn start_mock_ollama(plan_json: String) -> MockOllama {
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

pub async fn run_atomc(args: &[&str], dir: &Path, ollama_url: &str, input: Option<&str>) -> String {
    let args = args.iter().map(|value| value.to_string()).collect::<Vec<_>>();
    let dir = dir.to_path_buf();
    let ollama_url = ollama_url.to_string();
    let input = input.map(|value| value.to_string());

    tokio::task::spawn_blocking(move || run_atomc_sync(args, dir, ollama_url, input))
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
