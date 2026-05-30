use assert_cmd::Command;

#[test]
fn reports_running_then_exited() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args(["--name", "lifecycle-test", "--", "sleep", "0.5"]);
    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(stdout.contains("\"status\":\"starting\""), "missing starting: {}", stdout);
    assert!(stdout.contains("\"status\":\"running\""), "missing running: {}", stdout);

    let running_line = stdout
        .lines()
        .find(|l| l.contains("\"status\":\"running\""))
        .unwrap();
    assert!(running_line.contains("\"pid\":"), "missing pid in: {}", running_line);
    assert!(!running_line.contains("\"pid\":null"), "pid is null in running state");

    assert!(stdout.contains("\"status\":\"exited\""), "missing exited: {}", stdout);
}

#[test]
fn emits_valid_json_each_line() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args(["--name", "json-test", "--", "sleep", "0.3"]);
    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let _val: serde_json::Value =
            serde_json::from_str(line).expect(&format!("invalid json: {}", line));
    }
}

#[test]
fn exit_code_propagates() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args([
        "--name",
        "exit-code-test",
        "--",
        "sh",
        "-c",
        "exit 42",
    ]);
    cmd.assert().code(42);
}
