use assert_cmd::Command;

#[test]
fn reports_port_mapping() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args([
        "--name",
        "port-test",
        "--ports",
        "8080",
        "--ports",
        "8443:9443",
        "--",
        "sleep",
        "0.3",
    ]);
    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    let running_line = stdout
        .lines()
        .find(|l| l.contains("\"status\":\"running\""))
        .unwrap();

    assert!(
        running_line.contains("\"8080\""),
        "should contain port 8080 in: {}",
        running_line
    );
    assert!(
        running_line.contains("\"8443\""),
        "should contain port 8443 in: {}",
        running_line
    );
}

#[test]
fn assigns_host_port_for_container_only() {
    let mut cmd = Command::cargo_bin("podlet").unwrap();
    cmd.args([
        "--name",
        "port-auto-test",
        "--ports",
        "3000",
        "--",
        "sleep",
        "0.3",
    ]);
    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    let running_line = stdout
        .lines()
        .find(|l| l.contains("\"status\":\"running\""))
        .unwrap();

    let val: serde_json::Value = serde_json::from_str(running_line).unwrap();
    let port_str = val["ports"]["3000"].as_str().unwrap();
    let port: u16 = port_str.parse().unwrap();
    assert!(port > 0, "host port should be a positive integer, got: {}", port_str);
}
