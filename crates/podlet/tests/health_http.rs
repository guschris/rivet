use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread;

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

fn start_http_server(status: u16) -> (thread::JoinHandle<()>, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let response = format!("HTTP/1.0 {} OK\r\nContent-Length: 2\r\n\r\nok", status);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (handle, port)
}

#[test]
fn detects_http_healthy_with_200() {
    let (_handle, port) = start_http_server(200);

    let mut child = spawn_podlet(&[
        "--name",
        "http-health-test",
        "--http-check",
        &format!(":{}", port),
        "--health-interval",
        "500ms",
        "--",
        "sleep",
        "10",
    ]);

    let output = read_until(&mut child, "\"healthy\"");
    assert!(
        output.contains("\"healthy\""),
        "should have detected healthy, got: {}",
        output
    );

    child.kill().ok();
}

#[test]
fn detects_http_unhealthy_with_500() {
    let (_handle, port) = start_http_server(500);

    let mut child = spawn_podlet(&[
        "--name",
        "http-unhealthy-test",
        "--http-check",
        &format!(":{}", port),
        "--health-interval",
        "500ms",
        "--",
        "sleep",
        "10",
    ]);

    let output = read_until(&mut child, "\"unhealthy\"");
    assert!(
        output.contains("\"unhealthy\""),
        "should have detected unhealthy, got: {}",
        output
    );

    child.kill().ok();
}
