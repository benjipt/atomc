use atomc_core::llm::{LlmOptions, LlmError, OllamaClient, Prompt};
use axum::{extract::State, routing::post, Json, Router};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

struct ServerState {
    captured: Arc<Mutex<Option<Value>>>,
    response: Value,
    status: StatusCode,
    delay: Option<Duration>,
}

async fn spawn_server(
    response: Value,
    captured: Arc<Mutex<Option<Value>>>,
) -> (String, oneshot::Sender<()>) {
    spawn_server_with(response, captured, StatusCode::OK, None).await
}

async fn spawn_server_with(
    response: Value,
    captured: Arc<Mutex<Option<Value>>>,
    status: StatusCode,
    delay: Option<Duration>,
) -> (String, oneshot::Sender<()>) {
    let state = Arc::new(ServerState {
        captured,
        response,
        status,
        delay,
    });
    let app = Router::new()
        .route(
            "/api/generate",
            post(|State(state): State<Arc<ServerState>>, Json(payload): Json<Value>| {
                let state = state.clone();
                async move {
                    *state.captured.lock().unwrap() = Some(payload);
                    if let Some(delay) = state.delay {
                        tokio::time::sleep(delay).await;
                    }
                    (state.status, Json(state.response.clone()))
                }
            }),
        )
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    (format!("http://{addr}"), shutdown_tx)
}

#[tokio::test]
async fn ollama_client_parses_commit_plan() {
    let plan = json!({
        "schema_version": "v1",
        "plan": [
            {
                "id": "commit-1",
                "type": "docs",
                "scope": "cli",
                "summary": "document CLI plan and apply flags for usage examples",
                "body": ["Add usage examples", "Clarify diff input options"],
                "files": ["docs/02_cli_spec.md"],
                "hunks": []
            }
        ]
    });
    let response = json!({ "response": plan.to_string() });
    let captured = Arc::new(Mutex::new(None));
    let (base_url, shutdown) = spawn_server(response, captured.clone()).await;

    let client = OllamaClient::new(base_url);
    let prompt = Prompt {
        system: "system prompt".to_string(),
        user: "user prompt".to_string(),
    };
    let options = LlmOptions {
        model: "deepseek-coder".to_string(),
        temperature: 0.2,
        max_tokens: 128,
        timeout: Duration::from_secs(2),
    };

    let plan = client.generate_commit_plan(&prompt, &options).await.unwrap();
    assert_eq!(plan.schema_version, "v1");
    assert_eq!(plan.plan.len(), 1);

    let payload = captured.lock().unwrap().clone().expect("request captured");
    assert_eq!(payload["model"], "deepseek-coder");
    assert_eq!(payload["prompt"], "user prompt");
    assert_eq!(payload["system"], "system prompt");
    assert_eq!(payload["stream"], false);
    assert_eq!(
        payload["format"]["$schema"],
        json!("https://json-schema.org/draft/2020-12/schema")
    );
    assert_eq!(
        payload["format"]["$id"],
        json!("https://atomc.dev/schema/v1/commit-plan.json")
    );
    assert_eq!(payload["options"]["temperature"], json!(0.2));
    assert_eq!(payload["options"]["num_predict"], json!(128));

    let _ = shutdown.send(());
}

#[tokio::test]
async fn ollama_client_reports_non_success_status() {
    let response = json!({ "error": "runtime failure" });
    let captured = Arc::new(Mutex::new(None));
    let (base_url, shutdown) =
        spawn_server_with(response, captured, StatusCode::INTERNAL_SERVER_ERROR, None).await;

    let client = OllamaClient::new(base_url);
    let prompt = Prompt {
        system: "system prompt".to_string(),
        user: "user prompt".to_string(),
    };
    let options = LlmOptions {
        model: "deepseek-coder".to_string(),
        temperature: 0.2,
        max_tokens: 128,
        timeout: Duration::from_secs(2),
    };

    let error = client.generate_commit_plan(&prompt, &options).await.unwrap_err();
    assert!(matches!(error, LlmError::Runtime(_)));

    let _ = shutdown.send(());
}

#[tokio::test]
async fn ollama_client_rejects_invalid_json() {
    let response = json!({ "response": "not-json" });
    let captured = Arc::new(Mutex::new(None));
    let (base_url, shutdown) = spawn_server(response, captured).await;

    let client = OllamaClient::new(base_url);
    let prompt = Prompt {
        system: "system prompt".to_string(),
        user: "user prompt".to_string(),
    };
    let options = LlmOptions {
        model: "deepseek-coder".to_string(),
        temperature: 0.2,
        max_tokens: 128,
        timeout: Duration::from_secs(2),
    };

    let error = client.generate_commit_plan(&prompt, &options).await.unwrap_err();
    assert!(matches!(error, LlmError::Parse(_)));

    let _ = shutdown.send(());
}

#[tokio::test]
async fn ollama_client_times_out() {
    let response = json!({ "response": "{}" });
    let captured = Arc::new(Mutex::new(None));
    let (base_url, shutdown) =
        spawn_server_with(response, captured, StatusCode::OK, Some(Duration::from_millis(250)))
            .await;

    let client = OllamaClient::new(base_url);
    let prompt = Prompt {
        system: "system prompt".to_string(),
        user: "user prompt".to_string(),
    };
    let options = LlmOptions {
        model: "deepseek-coder".to_string(),
        temperature: 0.2,
        max_tokens: 128,
        timeout: Duration::from_millis(10),
    };

    let error = client.generate_commit_plan(&prompt, &options).await.unwrap_err();
    assert!(matches!(error, LlmError::Timeout));

    let _ = shutdown.send(());
}
