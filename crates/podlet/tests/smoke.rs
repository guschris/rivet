use assert_cmd::Command;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Workload supervisor"))
        .stdout(predicates::str::contains("--name"));
}

#[test]
fn rejects_empty_command() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.arg("--name").arg("test");
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no command specified"));
}

#[test]
fn runs_simple_command() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args(["--name", "smoke-test", "--", "echo", "hello"]);
    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(stdout.contains("\"status\":\"starting\""), "missing starting: {}", stdout);
    assert!(stdout.contains("\"status\":\"running\""), "missing running: {}", stdout);
    assert!(stdout.contains("\"status\":\"exited\""), "missing exited: {}", stdout);
}
