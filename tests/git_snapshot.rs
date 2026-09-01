use std::{fs, path::Path, process::Command};

use assert_fs::TempDir;
use serde_json::Value;

#[test]
fn git_snapshot_exposes_full_head_and_origin_without_hiding_missing_origin() {
    let repo = TempDir::new().expect("temporary repository");
    git(repo.path(), &["init"]);
    git(repo.path(), &["config", "user.name", "Dev CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "dev-cli@example.invalid"],
    );
    fs::write(repo.path().join("README.md"), "fixture\n").expect("write fixture");
    git(repo.path(), &["add", "README.md"]);
    git(repo.path(), &["commit", "-m", "test: snapshot fixture"]);
    git(
        repo.path(),
        &["remote", "add", "origin", "git@example.test:group/repo.git"],
    );

    let with_origin = snapshot(repo.path());
    let short = with_origin["head"].as_str().expect("short head");
    let full = with_origin["head_full"].as_str().expect("full head");
    assert!(full.starts_with(short));
    assert_eq!(full, git_output(repo.path(), &["rev-parse", "HEAD"]));
    assert_eq!(with_origin["origin_url"], "git@example.test:group/repo.git");
    assert!(with_origin["origin_error"].is_null());

    git(repo.path(), &["remote", "remove", "origin"]);
    let without_origin = snapshot(repo.path());
    assert!(without_origin["origin_url"].is_null());
    assert!(
        without_origin["origin_error"]
            .as_str()
            .is_some_and(|error| !error.is_empty())
    );
}

fn snapshot(repo: &Path) -> Value {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/scripts/git_snapshot.py");
    let output = Command::new("python3")
        .arg(script)
        .current_dir(repo)
        .output()
        .expect("run git snapshot fixture");
    assert!(
        output.status.success(),
        "snapshot failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("snapshot JSON")
}

fn git(repo: &Path, args: &[&str]) {
    let _ = git_output(repo, args);
}

fn git_output(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git fixture command");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git UTF-8 output")
        .trim()
        .to_owned()
}
