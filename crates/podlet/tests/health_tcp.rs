use std::io::{BufReader, Read};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn spawn_podlet(args: &[&str]) -> Child {
    let bin = env!("CARGO_BIN_EXE_podlet");
    Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn read_output(mut child: Child, timeout: Duration) -> (String, Child) {
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut output = String::new();
    let mut buf = [0u8; 4096];
    let deadline = std::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => output.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(_) => break,
        }
    }

    child.stdout = Some(reader.into_inner());
    (output, child)
}

#[test]
fn detects_tcp_healthy() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let mut child = spawn_podlet(&[
        "--name",
        "tcp-health-test",
        "--tcp-check",
        &format!(":{}", port),
        "--health-interval",
        "500ms",
        "--",
        "sleep",
        "5",
    ]);

    let (output, _child) = read_output(child, Duration::from_secs(2));
    child = _child;

    assert!(
        output.contains("\"healthy\""),
        "should have detected healthy, got: {}",
        output
    );

    drop(listener);
    child.kill().ok();
}

#[test]
fn detects_tcp_unhealthy_on_closed_port() {
    let mut child = spawn_podlet(&[
        "--name",
        "tcp-unhealthy-test",
        "--tcp-check",
        ":19999",
        "--health-interval",
        "500ms",
        "--",
        "sleep",
        "5",
    ]);

    let (output, _child) = read_output(child, Duration::from_secs(3));
    child = _child;

    assert!(
        output.contains("\"unhealthy\""),
        "should have detected unhealthy, got: {}",
        output
    );

    child.kill().ok();
}
