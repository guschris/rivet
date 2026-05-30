use assert_cmd::Command;
use predicates::str;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("sched").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(str::contains("Bin-packing scheduler"))
        .stdout(str::contains("--strategy"))
        .stdout(str::contains("--input"));
}

#[test]
fn schedules_first_fit_from_stdin() {
    let input = r#"{"nodes":["node1","node2","node3"],"loads":{"node1":5,"node2":2,"node3":4},"spec_name":"web","next_index":0}"#;

    let mut cmd = Command::cargo_bin("sched").unwrap();
    cmd.arg("--strategy").arg("first-fit");
    cmd.write_stdin(input);
    cmd.assert()
        .success()
        .stdout(str::contains("\"node\":\"node2\""))
        .stdout(str::contains("\"instance_id\":\"web-00000000\""));
}

#[test]
fn schedules_best_fit_from_stdin() {
    let input = r#"{"nodes":["node1","node2","node3"],"loads":{"node1":5,"node2":2,"node3":4},"spec_name":"api","next_index":7}"#;

    let mut cmd = Command::cargo_bin("sched").unwrap();
    cmd.arg("--strategy").arg("best-fit");
    cmd.write_stdin(input);
    cmd.assert()
        .success()
        .stdout(str::contains("\"node\":\"node2\""))
        .stdout(str::contains("\"instance_id\":\"api-00000007\""));
}

#[test]
fn reads_from_input_file() {
    let dir = tempfile::tempdir().unwrap();
    let input_path = dir.path().join("input.json");
    std::fs::write(
        &input_path,
        r#"{"nodes":["n1","n2"],"loads":{"n1":0,"n2":1},"spec_name":"svc","next_index":0}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("sched").unwrap();
    cmd.arg("--strategy").arg("first-fit")
        .arg("--input")
        .arg(&input_path);
    cmd.assert()
        .success()
        .stdout(str::contains("\"node\":\"n1\""));
}

#[test]
fn fails_on_empty_nodes() {
    let input = r#"{"nodes":[],"spec_name":"x"}"#;

    let mut cmd = Command::cargo_bin("sched").unwrap();
    cmd.arg("--strategy").arg("first-fit");
    cmd.write_stdin(input);
    cmd.assert().failure().stderr(str::contains("no available nodes"));
}

#[test]
fn rejects_invalid_strategy() {
    let input = r#"{"nodes":["a"],"spec_name":"x"}"#;

    let mut cmd = Command::cargo_bin("sched").unwrap();
    cmd.arg("--strategy").arg("round-robin");
    cmd.write_stdin(input);
    cmd.assert()
        .failure()
        .stderr(str::contains("unknown scheduler"));
}

#[test]
fn loads_default_to_zero_for_unmentioned_nodes() {
    let input = r#"{"nodes":["a","b","c"],"loads":{"b":10},"spec_name":"svc","next_index":0}"#;

    let mut cmd = Command::cargo_bin("sched").unwrap();
    cmd.arg("--strategy").arg("first-fit");
    cmd.write_stdin(input);
    cmd.assert()
        .success()
        .stdout(str::contains("\"node\":\"a\""));
}
