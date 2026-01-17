mod support;

use serde_json::Value;
use support::{run_atomc, start_mock_ollama};
use tempfile::TempDir;

struct GoldenCase {
    diff: &'static str,
    plan: &'static str,
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

fn load_fixture(relative: &str) -> String {
    let path = fixtures_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {}: {}", path.display(), err))
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
