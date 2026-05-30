mod exec;
mod leader;
mod reconciler;
mod scheduler;
mod spec;
mod state;

use clap::Parser;
use scheduler::Scheduler;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time;

#[derive(Parser, Debug)]
#[command(name = "flockd", about = "Declarative reconciler for infrastructure")]
struct Cli {
    #[arg(long)]
    specs: PathBuf,

    #[arg(long)]
    state: PathBuf,

    #[arg(long, default_value = "echo 'created {name} on {node}'")]
    exec_create: String,

    #[arg(long, default_value = "echo 'deleted {name} on {node}'")]
    exec_delete: String,

    #[arg(long, default_value = "")]
    exec_health: String,

    #[arg(long, default_value = "first-fit")]
    scheduler: String,

    #[arg(long)]
    nodes_file: Option<PathBuf>,

    #[arg(long, default_value = "")]
    node_health_cmd: String,

    #[arg(long, default_value = "5")]
    interval: u64,

    #[arg(long)]
    lock_file: Option<PathBuf>,

    #[arg(long)]
    max_load_per_node: Option<u32>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let scheduler = Scheduler::from_str(&cli.scheduler).unwrap_or_else(|e| {
        eprintln!("flockd: {}", e);
        std::process::exit(1);
    });

    let mut lock_guard = None;
    if let Some(ref lock_path) = cli.lock_file {
        match leader::try_acquire(lock_path) {
            Ok(Some(file)) => {
                eprintln!("flockd: acquired leader lock");
                lock_guard = Some(file);
            }
            Ok(None) => {
                eprintln!("flockd: another instance holds the lock, exiting");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("flockd: lock error: {}", e);
                std::process::exit(1);
            }
        }
    }

    let db = state::StateDB::open(&cli.state).unwrap_or_else(|e| {
        eprintln!("flockd: {}", e);
        if let Some(f) = lock_guard.take() {
            leader::release(f);
        }
        std::process::exit(1);
    });

    let mut ticker = time::interval(Duration::from_secs(cli.interval));
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    let mut last_spec_hashes: HashMap<String, String> = HashMap::new();
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to set up SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to set up SIGINT handler");

    loop {
        select! {
            _ = ticker.tick() => {}
            _ = sigterm.recv() => {
                eprintln!("flockd: received SIGTERM, shutting down");
                break;
            }
            _ = sigint.recv() => {
                eprintln!("flockd: received SIGINT, shutting down");
                break;
            }
        }

        let specs = match spec::load_specs(&cli.specs) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("flockd: error loading specs: {}", e);
                continue;
            }
        };

        let mut changed = false;
        for (name, s) in &specs {
            let hash = spec::hash_spec(s);
            let prev = last_spec_hashes.get(name).cloned().unwrap_or_default();
            if hash != prev {
                changed = true;
            }
            last_spec_hashes.insert(name.clone(), hash);
        }

        let nodes = match &cli.nodes_file {
            Some(path) => {
                let content = std::fs::read_to_string(path).unwrap_or_default();
                content
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .collect()
            }
            None => vec!["localhost".into()],
        };

        if !cli.node_health_cmd.is_empty() {
            let actions = reconciler::check_node_health(&db, &nodes, &cli.node_health_cmd);
            for action in &actions {
                eprintln!("flockd: {}", action);
            }
            _ = actions;
        }

        let actions = reconciler::reconcile(
            &db,
            &specs,
            &nodes,
            &scheduler,
            &cli.exec_create,
            &cli.exec_delete,
            &cli.exec_health,
        );

        for action in &actions {
            eprintln!("flockd: {}", action);
        }

        if actions.is_empty() && !changed {
            continue;
        }
    }

    eprintln!("flockd: reconciler stopped");
}
