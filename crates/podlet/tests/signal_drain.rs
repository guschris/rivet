use std::io::Read;
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

fn drain_and_read(child: &mut Child, wait_secs: u64) -> String {
    std::thread::sleep(Duration::from_secs(wait_secs));

    let mut stdout = child.stdout.take().unwrap();
    let mut output = String::new();
    stdout.read_to_string(&mut output).ok();
    child.wait().ok();
    output
}

#[test]
fn sigterm_triggers_drain_and_stop() {
    let mut child = spawn_podlet(&[
        "--name",
        "signal-test",
        "--drain-timeout",
        "5s",
        "--",
        "sleep",
        "30",
    ]);

    std::thread::sleep(Duration::from_secs(1));

    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    let output = drain_and_read(&mut child, 6);

    assert!(
        output.contains("\"draining\""),
        "should contain draining: {}",
        output
    );
    assert!(
        output.contains("\"stopped\""),
        "should contain stopped: {}",
        output
    );
}

#[test]
fn drain_kills_after_timeout() {
    let mut child = spawn_podlet(&[
        "--name",
        "drain-timeout-test",
        "--drain-timeout",
        "1s",
        "--",
        "sh",
        "-c",
        "trap '' TERM; sleep 30",
    ]);

    std::thread::sleep(Duration::from_secs(1));

    let start = std::time::Instant::now();
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    let output = drain_and_read(&mut child, 5);
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(900),
        "drain should have taken at least 1s, got {:?}",
        elapsed
    );
    assert!(output.contains("\"stopped\""), "should contain stopped: {}", output);
}
