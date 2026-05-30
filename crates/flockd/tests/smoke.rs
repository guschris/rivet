use assert_cmd::Command;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("flockd").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Declarative reconciler"))
        .stdout(predicates::str::contains("--specs"))
        .stdout(predicates::str::contains("--state"));
}

#[test]
fn requires_specs_and_state() {
    let mut cmd = Command::cargo_bin("flockd").unwrap();
    cmd.assert().failure();
}
