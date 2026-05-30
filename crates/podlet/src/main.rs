use clap::Parser;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time;

mod cgroups;
mod health;
mod workload;

#[derive(Parser, Debug)]
#[command(name = "podlet", about = "Workload supervisor for processes, containers, and VMs")]
struct Cli {
    #[arg(long)]
    name: String,

    #[arg(long, default_value = "process")]
    type_: String,

    #[arg(long)]
    cpu: Option<f64>,

    #[arg(long)]
    mem: Option<String>,

    #[arg(long = "ports", value_parser = parse_port)]
    port_mappings: Vec<PortMapping>,

    #[arg(long = "tcp-check")]
    tcp_check: Option<String>,

    #[arg(long = "http-check")]
    http_check: Option<String>,

    #[arg(long = "exec-check")]
    exec_check: Option<String>,

    #[arg(long)]
    restart: Option<String>,

    #[arg(long = "max-restarts", default_value = "10")]
    max_restarts: u32,

    #[arg(long = "drain-timeout", default_value = "30s", value_parser = parse_duration)]
    drain_timeout: Duration,

    #[arg(long = "health-interval", default_value = "5s", value_parser = parse_duration)]
    health_interval: Duration,

    #[arg(last = true, num_args = 0..)]
    command: Vec<String>,
}

#[derive(Debug, Clone)]
struct PortMapping {
    container_port: u16,
    host_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
struct State {
    status: String,
    pid: Option<u32>,
    health: String,
    ports: HashMap<String, String>,
}

fn parse_port(s: &str) -> Result<PortMapping, String> {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    let container_port: u16 = parts[0]
        .parse()
        .map_err(|_| format!("invalid container port: {}", parts[0]))?;

    let host_port = if parts.len() == 2 && !parts[1].is_empty() {
        Some(
            parts[1]
                .parse::<u16>()
                .map_err(|_| format!("invalid host port: {}", parts[1]))?,
        )
    } else {
        None
    };

    Ok(PortMapping {
        container_port,
        host_port,
    })
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }

    let (num_str, unit) = if let Some(s) = s.strip_suffix("ms") {
        (s.trim(), "ms")
    } else if let Some(s) = s.strip_suffix('s') {
        (s.trim(), "s")
    } else if let Some(s) = s.strip_suffix('m') {
        (s.trim(), "m")
    } else if let Some(s) = s.strip_suffix('h') {
        (s.trim(), "h")
    } else {
        return Err(format!("unknown duration unit in '{}', expected ms/s/m/h", s));
    };

    let num: f64 = num_str
        .parse()
        .map_err(|_| format!("invalid duration number: '{}'", num_str))?;

    if num < 0.0 {
        return Err(format!("negative duration not allowed: '{}'", s));
    }

    let secs = match unit {
        "ms" => num / 1000.0,
        "s" => num,
        "m" => num * 60.0,
        "h" => num * 3600.0,
        _ => unreachable!(),
    };

    Ok(Duration::from_secs_f64(secs))
}

fn parse_mem_bytes(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty memory value".into());
    }

    let (num_str, multiplier) = if let Some(s) = s.strip_suffix("Gi") {
        (s.trim(), 1024 * 1024 * 1024)
    } else if let Some(s) = s.strip_suffix("Mi") {
        (s.trim(), 1024 * 1024)
    } else if let Some(s) = s.strip_suffix("Ki") {
        (s.trim(), 1024)
    } else if let Some(s) = s.strip_suffix('G') {
        (s.trim(), 1000 * 1000 * 1000)
    } else if let Some(s) = s.strip_suffix('M') {
        (s.trim(), 1000 * 1000)
    } else if let Some(s) = s.strip_suffix('K') {
        (s.trim(), 1000)
    } else {
        (s, 1)
    };

    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("invalid memory number: '{}'", num_str))?;

    Ok(num * multiplier)
}

static STDOUT_BROKEN: AtomicBool = AtomicBool::new(false);

fn emit_state(state: &State) {
    if STDOUT_BROKEN.load(Ordering::Relaxed) {
        return;
    }
    let json = serde_json::to_string(state).unwrap();
    if writeln!(std::io::stdout(), "{}", json).is_err() {
        STDOUT_BROKEN.store(true, Ordering::Relaxed);
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.command.is_empty() {
        eprintln!("podlet: no command specified (use -- before the command)");
        std::process::exit(1);
    }

    if cli.type_ != "process" && cli.type_ != "container" && cli.type_ != "vm" {
        eprintln!("podlet: --type must be 'process', 'container', or 'vm'");
        std::process::exit(1);
    }

    let restart_policy = match cli.restart.as_deref() {
        None | Some("never") => workload::RestartPolicy::Never,
        Some("always") => workload::RestartPolicy::Always,
        Some("on-failure") => workload::RestartPolicy::OnFailure,
        Some(other) => {
            eprintln!("podlet: invalid --restart value '{}'", other);
            std::process::exit(1);
        }
    };

    let health_check = build_health_check(&cli);
    let mut restarts = 0u32;
    let cgroups_enabled = cli.cpu.is_some() || cli.mem.is_some();
    if cgroups_enabled && !std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        eprintln!("podlet: warning: cgroups v2 not available, resource limits will not be applied");
    }

    let mut exit_code: i32;

    let mut sigterm = signal(SignalKind::terminate()).expect("failed to set up SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to set up SIGINT handler");

    loop {
        let ports = match allocate_ports(&cli.port_mappings) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("podlet: {}", e);
                std::process::exit(1);
            }
        };
        let mut state = State {
            status: "starting".into(),
            pid: None,
            health: "unknown".into(),
            ports: port_map_to_json(&ports),
        };
        emit_state(&state);

        let mut child = match workload::spawn(&cli.type_, &cli.command, &ports, &cli.name) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("podlet: failed to spawn: {}", e);
                std::process::exit(1);
            }
        };

        let pid = child.id().unwrap();
        state.pid = Some(pid);
        state.status = "running".into();
        emit_state(&state);

        if let Some(ref hc) = health_check {
            if let Err(e) = hc.wait_ready(Duration::from_secs(2)).await {
                eprintln!("podlet: initial health check failed: {}", e);
            }
        }

        if cgroups_enabled {
            if let Some(ref cpu) = cli.cpu {
                if let Err(e) = cgroups::apply_cpu_limit(&cli.name, *cpu) {
                    eprintln!("podlet: cgroups cpu limit failed: {}", e);
                }
            }
            if let Some(ref mem) = cli.mem {
                if let Ok(bytes) = parse_mem_bytes(mem) {
                    if let Err(e) = cgroups::apply_mem_limit(&cli.name, bytes) {
                        eprintln!("podlet: cgroups mem limit failed: {}", e);
                    }
                }
            }
            if let Err(e) = cgroups::assign_pid(&cli.name, pid) {
                eprintln!("podlet: cgroups pid assignment failed: {}", e);
            }
        }

        let mut health_interval = time::interval(cli.health_interval);
        health_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        let child_exit_code = loop {
            select! {
                _ = child_exited(&mut child) => {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let code = status.code().unwrap_or(-1);
                            exit_code = code;
                            state.status = "exited".into();
                            state.pid = None;
                            state.health = "unknown".into();
                            emit_state(&state);
                            break code;
                        }
                        _ => continue,
                    }
                }
                _ = health_interval.tick() => {
                    if let Some(ref hc) = health_check {
                        let new_health = if hc.check().await {
                            "healthy"
                        } else {
                            "unhealthy"
                        };
                        if state.health != new_health {
                            state.health = new_health.into();
                            emit_state(&state);
                        }
                    }
                }
                _ = sigterm.recv() => {
                    state.status = "draining".into();
                    emit_state(&state);
                    drain_child(&mut child, cli.drain_timeout).await;
                    state.status = "stopped".into();
                    state.pid = None;
                    state.health = "unknown".into();
                    emit_state(&state);
                    return;
                }
                _ = sigint.recv() => {
                    state.status = "draining".into();
                    emit_state(&state);
                    drain_child(&mut child, cli.drain_timeout).await;
                    state.status = "stopped".into();
                    state.pid = None;
                    state.health = "unknown".into();
                    emit_state(&state);
                    return;
                }
            }
        };

        let should_restart = match restart_policy {
            workload::RestartPolicy::Always => true,
            workload::RestartPolicy::OnFailure => child_exit_code != 0,
            workload::RestartPolicy::Never => false,
        };

        if !should_restart {
            break;
        }

        restarts += 1;
        if restarts > cli.max_restarts {
            eprintln!("podlet: max restarts ({}) reached, giving up", cli.max_restarts);
            break;
        }

        eprintln!(
            "podlet: restarting {} (attempt {}/{})",
            cli.name, restarts, cli.max_restarts
        );
        time::sleep(Duration::from_secs(1)).await;
    }

    std::process::exit(exit_code);
}

fn build_health_check(cli: &Cli) -> Option<health::HealthChecker> {
    if let Some(ref port) = cli.tcp_check {
        let port = parse_check_port(port).unwrap_or_else(|e| {
            eprintln!("podlet: invalid tcp-check port: {}", e);
            std::process::exit(1);
        });
        Some(health::HealthChecker::tcp(port))
    } else if let Some(ref path) = cli.http_check {
        let (host, port, path) = parse_http_check(path).unwrap_or_else(|e| {
            eprintln!("podlet: invalid http-check: {}", e);
            std::process::exit(1);
        });
        Some(health::HealthChecker::http(host, port, path))
    } else {
        cli.exec_check
            .as_ref()
            .map(|cmd| health::HealthChecker::exec(cmd.clone()))
    }
}

fn parse_check_port(s: &str) -> Result<u16, String> {
    let s = s.trim().trim_start_matches(':');
    s.parse().map_err(|_| format!("invalid port: {}", s))
}

fn parse_http_check(s: &str) -> Result<(String, u16, String), String> {
    let s = s.trim();
    if let Some(path) = s.strip_prefix("http://") {
        let (host_port, path) = path.split_once('/').unwrap_or((path, ""));
        let (host, port_str) = host_port.split_once(':').unwrap_or((host_port, "80"));
        let port: u16 = port_str
            .parse()
            .map_err(|_| format!("invalid port: {}", port_str))?;
        let path = format!("/{}", path);
        Ok((host.to_string(), port, path))
    } else {
        let port: u16 = if let Some(without_colon) = s.strip_prefix(':') {
            without_colon
                .parse()
                .map_err(|_| format!("invalid port: {}", s))?
        } else {
            return Err(format!("http-check must start with ':' or 'http://', got '{}'", s));
        };
        let path = "/".to_string();
        Ok(("127.0.0.1".into(), port, path))
    }
}

fn allocate_random_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .map(|l| l.local_addr().unwrap().port())
        .map_err(|e| format!("failed to allocate port: {}", e))
}

fn allocate_ports(mappings: &[PortMapping]) -> Result<Vec<(PortMapping, u16)>, String> {
    mappings
        .iter()
        .map(|m| {
            let host_port = match m.host_port {
                Some(p) => p,
                None => allocate_random_port()?,
            };
            Ok((m.clone(), host_port))
        })
        .collect()
}

fn port_map_to_json(ports: &[(PortMapping, u16)]) -> HashMap<String, String> {
    ports
        .iter()
        .map(|(mapping, host_port)| {
            (
                mapping.container_port.to_string(),
                host_port.to_string(),
            )
        })
        .collect()
}

async fn drain_child(child: &mut tokio::process::Child, drain_timeout: Duration) {
    let pid = child.id().unwrap();

    if child.try_wait().ok().flatten().is_some() {
        eprintln!("podlet: process {} already exited", pid);
        return;
    }

    unsafe { libc::kill(pid as i32, libc::SIGTERM); }
    eprintln!("podlet: sent SIGTERM to pid {}", pid);

    let deadline = time::Instant::now() + drain_timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                eprintln!("podlet: process {} exited during drain", pid);
                return;
            }
            Ok(None) => {
                if time::Instant::now() >= deadline {
                    break;
                }
                time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                eprintln!("podlet: error waiting for process {}: {}", pid, e);
                time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    eprintln!("podlet: drain timeout, sending SIGKILL to pid {}", pid);
    child.start_kill().ok();
    let _ = child.wait().await;
}

async fn child_exited(child: &mut tokio::process::Child) {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => time::sleep(Duration::from_millis(200)).await,
            Err(_) => return,
        }
    }
}
