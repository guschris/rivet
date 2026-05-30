use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn restart_on_failure_retries() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args([
        "--name",
        "restart-test",
        "--restart",
        "on-failure",
        "--max-restarts",
        "2",
        "--health-interval",
        "10s",
        "--",
        "sh",
        "-c",
        "exit 1",
    ]);
    let output = cmd.assert().code(predicate::ne(0));
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    let restart_count = stderr.matches("restarting").count();
    assert_eq!(restart_count, 2, "should restart exactly 2 times, got stderr: {}", stderr);
    assert!(
        stderr.contains("max restarts"),
        "should mention max restarts: {}",
        stderr
    );
}

#[test]
fn restart_always_retries() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args([
        "--name",
        "restart-always-test",
        "--restart",
        "always",
        "--max-restarts",
        "2",
        "--health-interval",
        "10s",
        "--",
        "sh",
        "-c",
        "exit 0",
    ]);
    let output = cmd.assert().success();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    assert!(
        stderr.contains("max restarts"),
        "with --restart always and max 2, should eventually give up: {}",
        stderr
    );
}

#[test]
fn no_restart_by_default() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args([
        "--name",
        "no-restart-test",
        "--",
        "sh",
        "-c",
        "exit 1",
    ]);
    let output = cmd.assert().code(predicate::eq(1));
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    assert!(
        !stderr.contains("restarting"),
        "should not restart by default: {}",
        stderr
    );
}
