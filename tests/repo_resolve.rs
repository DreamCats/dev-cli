use std::{fs, path::Path, process::Command};

use assert_fs::TempDir;
use serde_json::Value;

#[test]
fn repo_resolve_handles_direct_and_root_relative_paths() {
    let fixture = TempDir::new().expect("temporary fixture");
    let direct = fixture.path().join("direct");
    let root = fixture.path().join("root");
    let rooted = root.join("group/repo");
    fs::create_dir_all(&direct).unwrap();
    fs::create_dir_all(&rooted).unwrap();

    let direct_result = resolve(direct.to_str().unwrap(), &[]);
    assert_eq!(direct_result["source"], "direct");
    assert_eq!(direct_result["path"], direct.to_str().unwrap());

    let rooted_result = resolve("group/repo", &[root.to_str().unwrap()]);
    assert_eq!(rooted_result["source"], "root");
    assert_eq!(rooted_result["path"], rooted.to_str().unwrap());
}

fn resolve(repo: &str, roots: &[&str]) -> Value {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/scripts/repo_resolve.py");
    let output = Command::new("python3")
        .arg(script)
        .arg(repo)
        .arg(serde_json::to_string(roots).unwrap())
        .output()
        .expect("run repo resolve fixture");
    assert!(
        output.status.success(),
        "repo resolve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("repo resolve JSON")
}
