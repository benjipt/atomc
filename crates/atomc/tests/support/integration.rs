use crate::support::atomc_bin;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};

pub struct AtomcServer {
    pub base_url: String,
    child: Child,
}

impl Drop for AtomcServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn init_repo_with_change() -> TempDir {
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

pub fn run_git(dir: &Path, args: &[&str]) -> String {
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

pub fn reserve_port() -> u16 {
    StdTcpListener::bind("127.0.0.1:0")
        .expect("reserve port")
        .local_addr()
        .expect("port addr")
        .port()
}

pub fn start_atomc_server(port: u16, ollama_url: &str) -> AtomcServer {
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
        .env("LOCAL_COMMIT_LLM_TIMEOUT_SECS", "5")
        .env_remove("LOCAL_COMMIT_AGENT_CONFIG")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().expect("spawn atomc serve");
    AtomcServer {
        base_url: format!("http://127.0.0.1:{port}"),
        child,
    }
}

pub async fn wait_for_port(port: u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    for _ in 0..40 {
        if TcpStream::connect(addr).await.is_ok() {
            return;
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not start on port {port}");
}
