use std::io::{BufRead, BufReader};
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

fn read_until(child: &mut Child, predicate: &str) -> String {
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut output = String::new();
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                output.push_str(&line);
                if line.contains(predicate) {
                    break;
                }
            }
            Err(_) => break,
        }
        if output.lines().count() > 100 {
            break;
        }
    }

    child.stdout = Some(reader.into_inner());
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

    let output = read_until(&mut child, "\"status\":\"running\"");
    assert!(output.contains("\"status\":\"running\""));

    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    let output = read_until(&mut child, "\"stopped\"");
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

    child.wait().ok();
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

    let output = read_until(&mut child, "\"status\":\"running\"");
    assert!(output.contains("\"status\":\"running\""));

    let start = std::time::Instant::now();
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    let output = read_until(&mut child, "\"stopped\"");
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(900),
        "drain should have taken at least 1s, got {:?}",
        elapsed
    );
    assert!(output.contains("\"stopped\""));

    child.wait().ok();
}
