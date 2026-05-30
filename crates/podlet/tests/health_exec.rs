use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

fn spawn_podlet(args: &[&str]) -> Child {
    let bin = env!("CARGO_BIN_EXE_podlet");
    Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn read_all(child: &mut Child) -> String {
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut output = String::new();
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => output.push_str(&line),
            Err(_) => break,
        }
    }

    child.stdout = Some(reader.into_inner());
    output
}

#[test]
fn exec_check_healthy_on_success() {
    let mut child = spawn_podlet(&[
        "--name",
        "exec-health-test",
        "--exec-check",
        "true",
        "--health-interval",
        "500ms",
        "--",
        "sleep",
        "3",
    ]);

    let output = read_all(&mut child);
    child.wait().ok();

    assert!(
        output.contains("\"healthy\""),
        "should have detected healthy, got: {}",
        output
    );
}

#[test]
fn exec_check_unhealthy_on_failure() {
    let mut child = spawn_podlet(&[
        "--name",
        "exec-unhealthy-test",
        "--exec-check",
        "false",
        "--health-interval",
        "500ms",
        "--",
        "sleep",
        "3",
    ]);

    let output = read_all(&mut child);
    child.wait().ok();

    assert!(
        output.contains("\"unhealthy\""),
        "should have detected unhealthy, got: {}",
        output
    );
}
