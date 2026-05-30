use clap::Parser;
use health_lib::HealthChecker;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task::JoinSet;

#[derive(Parser, Debug)]
#[command(
    name = "probe",
    about = "Async health-checker; reads targets and streams NDJSON status"
)]
struct Cli {
    #[arg(long, value_name = "FILE", help = "JSON file with target definitions")]
    targets: PathBuf,

    #[arg(long, default_value = "5", help = "Check interval in seconds")]
    interval: u64,

    #[arg(
        long,
        default_value = "json",
        help = "Output format: json (NDJSON) or text"
    )]
    format: String,

    #[arg(long, help = "Run one check cycle and exit (default: loop forever)")]
    once: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Target {
    name: String,
    #[serde(rename = "type")]
    check_type: String,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    command: Option<String>,
}

#[derive(serde::Serialize)]
struct ProbeResult {
    target: String,
    #[serde(rename = "type")]
    result_type: String,
    healthy: bool,
    elapsed_ms: u128,
    ts_ms: u128,
}

fn load_targets(path: &PathBuf) -> Result<Vec<Target>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    serde_json::from_str(&content).map_err(|e| format!("{}: invalid JSON: {}", path.display(), e))
}

fn build_checker(target: &Target) -> Result<HealthChecker, String> {
    match target.check_type.as_str() {
        "tcp" => {
            let host = target.host.as_deref().unwrap_or("127.0.0.1");
            let port = target
                .port
                .ok_or_else(|| format!("target '{}': tcp check requires port", target.name))?;
            Ok(HealthChecker::tcp(host.to_string(), port))
        }
        "http" => {
            let host = target.host.as_deref().unwrap_or("127.0.0.1");
            let port = target.port.ok_or_else(|| {
                format!("target '{}': http check requires port", target.name)
            })?;
            let path = target.path.as_deref().unwrap_or("/");
            Ok(HealthChecker::http(
                host.to_string(),
                port,
                path.to_string(),
            ))
        }
        "exec" => {
            let command = target.command.as_deref().ok_or_else(|| {
                format!("target '{}': exec check requires command", target.name)
            })?;
            Ok(HealthChecker::exec(command.to_string()))
        }
        other => Err(format!(
            "target '{}': unknown check type '{}' (expected tcp, http, or exec)",
            target.name, other
        )),
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

async fn run_cycle(checkers: &[(String, String, HealthChecker)], format: &str) {
    let mut set = JoinSet::new();
    for (name, check_type, checker) in checkers {
        let name = name.clone();
        let check_type = check_type.clone();
        let checker = checker.clone();
        set.spawn(async move {
            let start = now_ms();
            let healthy = checker.check().await;
            let elapsed = now_ms() - start;

            ProbeResult {
                target: name,
                result_type: check_type,
                healthy,
                elapsed_ms: elapsed,
                ts_ms: start + elapsed,
            }
        });
    }

    while let Some(res) = set.join_next().await {
        match res {
            Ok(result) => {
                if format == "text" {
                    println!(
                        "{} {} {}ms",
                        result.target,
                        if result.healthy { "UP" } else { "DOWN" },
                        result.elapsed_ms,
                    );
                } else {
                    let line =
                        serde_json::to_string(&result).unwrap_or_else(|_| "{}".into());
                    println!("{}", line);
                }
            }
            Err(e) => {
                eprintln!("probe: task error: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let targets = load_targets(&cli.targets).unwrap_or_else(|e| {
        eprintln!("probe: error: {}", e);
        std::process::exit(1);
    });

    let checkers: Vec<(String, String, HealthChecker)> = targets
        .iter()
        .map(|t| build_checker(t).map(|hc| (t.name.clone(), t.check_type.clone(), hc)))
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| {
            eprintln!("probe: error: {}", e);
            std::process::exit(1);
        });

    if cli.once {
        run_cycle(&checkers, &cli.format).await;
        return;
    }

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(cli.interval));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        run_cycle(&checkers, &cli.format).await;
    }
}
