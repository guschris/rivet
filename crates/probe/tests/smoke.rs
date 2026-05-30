use assert_cmd::Command;
use predicates::str;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(str::contains("Async health-checker"))
        .stdout(str::contains("--targets"))
        .stdout(str::contains("--interval"))
        .stdout(str::contains("--once"));
}

#[test]
fn rejects_missing_targets_file() {
    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg("/nonexistent/targets.json");
    cmd.assert().failure().stderr(str::contains("error"));
}

#[test]
fn rejects_invalid_json() {
    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(&targets, "not json").unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg(&targets).arg("--once");
    cmd.assert().failure().stderr(str::contains("invalid JSON"));
}

#[test]
fn runs_exec_check_once() {
    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(
        &targets,
        r#"[{"name": "test", "type": "exec", "command": "true"}]"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg(&targets).arg("--once");
    cmd.assert()
        .success()
        .stdout(str::contains("\"target\":\"test\""))
        .stdout(str::contains("\"type\":\"exec\""))
        .stdout(str::contains("\"healthy\":true"));
}

#[test]
fn detects_unhealthy_exec() {
    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(
        &targets,
        r#"[{"name": "fail", "type": "exec", "command": "false"}]"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg(&targets).arg("--once");
    cmd.assert()
        .success()
        .stdout(str::contains("\"healthy\":false"));
}

#[test]
fn tcp_check_closed_port() {
    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(
        &targets,
        r#"[{"name": "dead", "type": "tcp", "host": "127.0.0.1", "port": 19}]"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg(&targets).arg("--once");
    cmd.assert()
        .success()
        .stdout(str::contains("\"healthy\":false"));
}

#[test]
fn tcp_check_open_port() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(
        &targets,
        format!(
            r#"[{{"name": "alive", "type": "tcp", "host": "127.0.0.1", "port": {}}}]"#,
            port
        ),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg(&targets).arg("--once");
    cmd.assert()
        .success()
        .stdout(str::contains("\"healthy\":true"));

    drop(listener);
}

#[test]
fn text_format_output() {
    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(
        &targets,
        r#"[{"name": "check", "type": "exec", "command": "true"}]"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets")
        .arg(&targets)
        .arg("--once")
        .arg("--format")
        .arg("text");
    cmd.assert()
        .success()
        .stdout(str::contains("UP"))
        .stdout(str::contains("ms"));
}

#[test]
fn rejects_unknown_check_type() {
    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(
        &targets,
        r#"[{"name": "bad", "type": "grpc", "port": 50051}]"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg(&targets).arg("--once");
    cmd.assert()
        .failure()
        .stderr(str::contains("unknown check type"));
}

#[test]
fn multiple_targets_concurrently() {
    let dir = tempfile::tempdir().unwrap();
    let targets = dir.path().join("targets.json");
    std::fs::write(
        &targets,
        r#"[
            {"name": "a", "type": "exec", "command": "true"},
            {"name": "b", "type": "exec", "command": "true"}
        ]"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("probe").unwrap();
    cmd.arg("--targets").arg(&targets).arg("--once");
    cmd.assert()
        .success()
        .stdout(str::contains("\"target\":\"a\""))
        .stdout(str::contains("\"target\":\"b\""));
}
