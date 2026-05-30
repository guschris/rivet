use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("merge").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(str::contains("Deep-merge"))
        .stdout(str::contains("BASE"))
        .stdout(str::contains("--patch"))
        .stdout(str::contains("--format"));
}

#[test]
fn merges_yaml_files() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.yaml");
    let patch = dir.path().join("patch.yaml");
    std::fs::write(
        &base,
        "key1: value1\nnested:\n  a: 1\n  b: 2\narr: [1, 2]\n",
    )
    .unwrap();
    std::fs::write(
        &patch,
        "key2: value2\nnested:\n  b: 99\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("merge").unwrap();
    cmd.arg(base.to_str().unwrap())
        .arg("--patch")
        .arg(patch.to_str().unwrap());
    cmd.assert()
        .success()
        .stdout(str::contains("key1: value1"))
        .stdout(str::contains("key2: value2"))
        .stdout(str::contains("b: 99"))
        .stdout(str::contains("a: 1"))
        .stdout(str::contains("arr:"));
}

#[test]
fn merges_json_files() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.json");
    let patch = dir.path().join("patch.json");
    std::fs::write(&base, r#"{"name":"app","db":{"host":"localhost","port":5432}}"#).unwrap();
    std::fs::write(
        &patch,
        r#"{"db":{"host":"prod-db.example.com"},"debug":true}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("merge").unwrap();
    cmd.arg(base.to_str().unwrap())
        .arg("--patch")
        .arg(patch.to_str().unwrap());
    cmd.assert()
        .success()
        .stdout(str::contains("\"name\": \"app\""))
        .stdout(str::contains("\"host\": \"prod-db.example.com\""))
        .stdout(str::contains("\"port\": 5432"))
        .stdout(str::contains("\"debug\": true"));
}

#[test]
fn null_deletes_key() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.yaml");
    let patch = dir.path().join("patch.yaml");
    std::fs::write(&base, "secret: abc123\nkeep: yes\nnested:\n  old: x\n  keep: y\n").unwrap();
    std::fs::write(&patch, "secret: null\nnested:\n  old: null\n  new: z\n").unwrap();

    let mut cmd = Command::cargo_bin("merge").unwrap();
    cmd.arg(base.to_str().unwrap())
        .arg("--patch")
        .arg(patch.to_str().unwrap());
    cmd.assert()
        .success()
        .stdout(str::contains("keep: yes"))
        .stdout(str::contains("secret:").not())
        .stdout(str::contains("nested:"))
        .stdout(str::contains("new: z"));
}

#[test]
fn patch_array_replaces_wholesale() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.yaml");
    let patch = dir.path().join("patch.yaml");
    std::fs::write(&base, "ports:\n  - 111\n  - 222\nother: kept\n").unwrap();
    std::fs::write(&patch, "ports:\n  - 333\n").unwrap();

    let mut cmd = Command::cargo_bin("merge").unwrap();
    cmd.arg(base.to_str().unwrap())
        .arg("--patch")
        .arg(patch.to_str().unwrap());
    cmd.assert()
        .success()
        .stdout(str::contains("- 333"))
        .stdout(str::contains("kept"))
        .stdout(str::contains("- 111").not())
        .stdout(str::contains("- 222").not());
}

#[test]
fn error_on_missing_patch_file() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.yaml");
    std::fs::write(&base, "key: value\n").unwrap();

    let mut cmd = Command::cargo_bin("merge").unwrap();
    cmd.arg(base.to_str().unwrap())
        .arg("--patch")
        .arg("/nonexistent/patch.yaml");
    cmd.assert().failure().stderr(str::contains("error"));
}

#[test]
fn explicit_json_output_from_yaml_input() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.yaml");
    let patch = dir.path().join("patch.yaml");
    std::fs::write(&base, "name: test\ncount: 1\n").unwrap();
    std::fs::write(&patch, "count: 5\n").unwrap();

    let mut cmd = Command::cargo_bin("merge").unwrap();
    cmd.arg(base.to_str().unwrap())
        .arg("--patch")
        .arg(patch.to_str().unwrap())
        .arg("--format")
        .arg("json");
    cmd.assert()
        .success()
        .stdout(str::contains("\"name\": \"test\""))
        .stdout(str::contains("\"count\": 5"));
}
