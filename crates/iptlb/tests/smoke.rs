use assert_cmd::Command;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("iptlb").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("File-driven L4 load balancer"))
        .stdout(predicates::str::contains("--vip"))
        .stdout(predicates::str::contains("--port"))
        .stdout(predicates::str::contains("--backends-file"));
}

#[test]
fn rejects_invalid_scheduler() {
    let mut cmd = Command::cargo_bin("iptlb").unwrap();
    cmd.args([
        "--vip", "10.0.0.1",
        "--port", "80",
        "--backends-file", "/nonexistent",
        "--scheduler", "wrr",
        "--interval", "60",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("only 'rr'"));
}
