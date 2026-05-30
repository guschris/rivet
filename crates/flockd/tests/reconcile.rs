use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn spawn_flockd(specs_dir: &str, state_db: &str, nodes_file: Option<&str>) -> Child {
    let bin = env!("CARGO_BIN_EXE_flockd");
    let mut cmd = Command::new(bin);
    cmd.args([
        "--specs", specs_dir,
        "--state", state_db,
        "--exec-create", "echo 'created {name} on {node}'",
        "--exec-delete", "echo 'deleted {name} on {node}'",
        "--exec-health", "true",
        "--interval", "1",
    ]);
    if let Some(nf) = nodes_file {
        cmd.args(["--nodes-file", nf]);
    }
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn kill_and_read_stderr(child: &mut Child) -> String {
    child.kill().ok();
    let mut stderr = child.stderr.take().unwrap();
    use std::io::Read;
    let mut output = String::new();
    stderr.read_to_string(&mut output).ok();
    child.wait().ok();
    output
}

#[test]
fn creates_instances_to_meet_desired_replicas() {
    let dir = tempfile::tempdir().unwrap();
    let specs_dir = dir.path().join("specs");
    std::fs::create_dir(&specs_dir).unwrap();
    let state_db = dir.path().join("state.db");
    let nodes_file = dir.path().join("nodes");
    std::fs::write(&nodes_file, "node1\nnode2\n").unwrap();

    std::fs::write(
        specs_dir.join("app.yaml"),
        "name: app\nreplicas: 2\n",
    ).unwrap();

    let mut child = spawn_flockd(
        specs_dir.to_str().unwrap(),
        state_db.to_str().unwrap(),
        Some(nodes_file.to_str().unwrap()),
    );

    std::thread::sleep(Duration::from_secs(5));
    let output = kill_and_read_stderr(&mut child);

    let create_count = output.matches("created:").count();
    assert!(
        create_count >= 2,
        "should have created 2 instances, got {} creates. output:\n{}",
        create_count,
        output
    );
}

#[test]
fn starts_and_stays_running() {
    let dir = tempfile::tempdir().unwrap();
    let specs_dir = dir.path().join("specs");
    std::fs::create_dir(&specs_dir).unwrap();
    let state_db = dir.path().join("state.db");
    let nodes_file = dir.path().join("nodes");
    std::fs::write(&nodes_file, "node1\n").unwrap();

    std::fs::write(
        specs_dir.join("app.yaml"),
        "name: app\nreplicas: 1\n",
    ).unwrap();

    let mut child = spawn_flockd(
        specs_dir.to_str().unwrap(),
        state_db.to_str().unwrap(),
        Some(nodes_file.to_str().unwrap()),
    );

    std::thread::sleep(Duration::from_secs(3));

    match child.try_wait() {
        Ok(Some(status)) => panic!("flockd exited early with status {:?}", status),
        Ok(None) => {}
        Err(e) => panic!("try_wait error: {}", e),
    }

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn scale_down_removes_excess() {
    let dir = tempfile::tempdir().unwrap();
    let specs_dir = dir.path().join("specs");
    std::fs::create_dir(&specs_dir).unwrap();
    let state_db = dir.path().join("state.db");
    let nodes_file = dir.path().join("nodes");
    std::fs::write(&nodes_file, "node1\nnode2\n").unwrap();

    std::fs::write(
        specs_dir.join("app.yaml"),
        "name: app\nreplicas: 3\n",
    ).unwrap();

    let mut child = spawn_flockd(
        specs_dir.to_str().unwrap(),
        state_db.to_str().unwrap(),
        Some(nodes_file.to_str().unwrap()),
    );

    std::thread::sleep(Duration::from_secs(6));

    std::fs::write(
        specs_dir.join("app.yaml"),
        "name: app\nreplicas: 1\n",
    ).unwrap();

    std::thread::sleep(Duration::from_secs(6));

    let output = kill_and_read_stderr(&mut child);

    assert!(
        output.contains("created:"),
        "should have created instances:\n{}",
        output
    );

    assert!(
        output.contains("excess") || output.contains("delete:") || output.contains("drain/delete old:"),
        "should detect excess and delete:\n{}",
        output
    );
}

#[test]
fn state_persists_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let specs_dir = dir.path().join("specs");
    std::fs::create_dir(&specs_dir).unwrap();
    let state_db = dir.path().join("state.db");
    let nodes_file = dir.path().join("nodes");
    std::fs::write(&nodes_file, "node1\n").unwrap();

    std::fs::write(
        specs_dir.join("app.yaml"),
        "name: app\nreplicas: 1\n",
    ).unwrap();

    let mut child1 = spawn_flockd(
        specs_dir.to_str().unwrap(),
        state_db.to_str().unwrap(),
        Some(nodes_file.to_str().unwrap()),
    );

    std::thread::sleep(Duration::from_secs(3));
    let output1 = kill_and_read_stderr(&mut child1);
    assert!(output1.contains("created:"), "first run should create");

    let mut child2 = spawn_flockd(
        specs_dir.to_str().unwrap(),
        state_db.to_str().unwrap(),
        Some(nodes_file.to_str().unwrap()),
    );

    std::thread::sleep(Duration::from_secs(5));
    let output2 = kill_and_read_stderr(&mut child2);

    assert!(
        output2.contains("no change") || output2.contains("1 replicas healthy"),
        "second run should detect no change needed:\n{}",
        output2
    );
}
