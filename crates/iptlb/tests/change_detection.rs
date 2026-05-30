use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn spawn_iptlb(backends_file: &str) -> Child {
    let bin = env!("CARGO_BIN_EXE_iptlb");
    Command::new(bin)
        .args([
            "--vip", "10.0.0.1",
            "--port", "80",
            "--backends-file", backends_file,
            "--interval", "1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait_for_output(child: &mut Child, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        match child.try_wait() {
            Ok(Some(_)) => return false,
            Ok(None) => {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(_) => return false,
        }
    }
    true
}

#[test]
fn starts_without_crashing_with_valid_backends() {
    let dir = tempfile::tempdir().unwrap();
    let backends_path = dir.path().join("backends.txt");
    std::fs::write(&backends_path, "10.0.0.2:8080\n").unwrap();

    let mut child = spawn_iptlb(backends_path.to_str().unwrap());

    let still_running = wait_for_output(&mut child, 3);
    assert!(still_running, "iptlb should still be running after 3 seconds");

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn handles_nonexistent_backends_file() {
    let mut child = spawn_iptlb("/tmp/definitely-does-not-exist-backends-file-12345");

    let still_running = wait_for_output(&mut child, 3);
    assert!(still_running, "iptlb should stay alive even with missing backends file");

    child.kill().ok();
    child.wait().ok();
}
