use std::{fs, path::Path};

use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

fn dev() -> Command {
    Command::cargo_bin("dev").expect("binary builds")
}

fn config_root() -> TempDir {
    TempDir::new().expect("temporary config root")
}

#[test]
fn exposes_the_complete_go_command_surface() {
    let output = dev().arg("--help").output().expect("help runs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in [
        "ls",
        "cat",
        "history",
        "slice",
        "grep",
        "find",
        "tree",
        "head",
        "tail",
        "push",
        "pull",
        "exec",
        "exec-watch",
        "write",
        "edit",
        "diff",
        "patch",
        "repo-status",
        "repo-diff",
        "git-snapshot",
        "repo",
        "verify",
        "cg",
        "config",
        "stats",
        "version",
    ] {
        assert!(
            stdout.contains(command),
            "help missing {command}:\n{stdout}"
        );
    }
}

#[test]
fn config_round_trips_the_existing_yaml_and_json_contract() {
    let root = config_root();
    dev()
        .env("XDG_CONFIG_HOME", root.path())
        .args([
            "config",
            "add",
            "sgdev",
            "10.0.0.1",
            "--user",
            "maifeng",
            "--default",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "已添加主机: sgdev (maifeng@10.0.0.1)",
        ));

    let yaml = fs::read_to_string(root.path().join("dev-connect/config.yaml")).unwrap();
    for expected in [
        "default_host: sgdev",
        "os: null",
        "shell: null",
        "exec_timeout: null",
        "repo_roots: []",
    ] {
        assert!(
            yaml.contains(expected),
            "yaml missing {expected:?}:\n{yaml}"
        );
    }

    dev()
        .env("XDG_CONFIG_HOME", root.path())
        .args(["config", "show", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"default_host\": \"sgdev\""))
        .stdout(predicate::str::contains("\"os\": \"posix\""))
        .stdout(predicate::str::contains("\"shell\": null"))
        .stdout(predicate::str::contains("\"repo_roots\": []"));
}

#[test]
fn history_records_only_redacted_command_metadata() {
    let root = config_root();
    dev()
        .env("XDG_CONFIG_HOME", root.path())
        .env("DEV_SESSION_ID", "test-session")
        .args(["version"])
        .assert()
        .success();
    dev()
        .env("XDG_CONFIG_HOME", root.path())
        .args(["--json", "history", "--limit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"version\""))
        .stdout(predicate::str::contains("\"session_id\": \"test-session\""));
    let history = fs::read_to_string(root.path().join("dev-connect/history.jsonl")).unwrap();
    assert!(!history.contains("--limit"));
}

#[test]
fn exec_and_grep_preserve_json_and_fail_loud_contracts() {
    let root = config_root();
    let fake = TempDir::new().unwrap();
    let calls = fake.path().join("calls.txt");
    write_fake_ssh(fake.path(), &calls);
    let path = format!(
        "{}:{}",
        fake.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    dev()
        .env("XDG_CONFIG_HOME", root.path())
        .args(["config", "add", "fake", "example.test", "--default"])
        .assert()
        .success();

    dev()
        .env("XDG_CONFIG_HOME", root.path())
        .env("PATH", &path)
        .env("FAKE_SSH_CALLS", &calls)
        .env("FAKE_SSH_STDOUT", "hello\\n")
        .args(["--json", "exec", "--", "echo", "ok"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"echo ok\""))
        .stdout(predicate::str::contains("\"success\": true"));

    dev()
        .env("XDG_CONFIG_HOME", root.path())
        .env("PATH", &path)
        .env("FAKE_SSH_CALLS", &calls)
        .env("FAKE_SSH_STDOUT", "a.go:2:needle\\n")
        .args(["--json", "grep", "needle", "."])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"tool\": \"grep\""))
        .stdout(predicate::str::contains("\"count\": 1"));
}

fn write_fake_ssh(directory: &Path, calls: &Path) {
    let script = directory.join("ssh");
    fs::write(
        &script,
        r#"#!/bin/sh
last=""
for arg in "$@"; do last="$arg"; done
printf '%s\n' "$last" >> "$FAKE_SSH_CALLS"
if [ "$last" = "which rg" ]; then exit 1; fi
printf '%b' "$FAKE_SSH_STDOUT"
exit "${FAKE_SSH_EXIT:-0}"
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let _ = calls;
}
