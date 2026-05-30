use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

pub enum HealthChecker {
    Tcp { port: u16 },
    Http { host: String, port: u16, path: String },
    Exec { command: String },
}

impl HealthChecker {
    pub fn tcp(port: u16) -> Self {
        HealthChecker::Tcp { port }
    }

    pub fn http(host: String, port: u16, path: String) -> Self {
        HealthChecker::Http { host, port, path }
    }

    pub fn exec(command: String) -> Self {
        HealthChecker::Exec { command }
    }

    pub async fn check(&self) -> bool {
        match self {
            HealthChecker::Tcp { port } => check_tcp(*port),
            HealthChecker::Http { host, port, path } => check_http(host, *port, path),
            HealthChecker::Exec { command } => check_exec(command),
        }
    }

    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if self.check().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Err(format!("health check did not become healthy within {:?}", timeout))
    }
}

fn check_tcp(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok()
}

fn check_http(host: &str, port: u16, path: &str) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let request = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );

    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut reader = BufReader::new(&mut stream);
    let mut status_line = String::new();
    if reader.read_line(&mut status_line).is_err() {
        return false;
    }

    let status_parts: Vec<&str> = status_line.split_whitespace().collect();
    if status_parts.len() < 2 {
        return false;
    }

    matches!(status_parts[1].chars().next(), Some('2') | Some('3'))
}

fn check_exec(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
