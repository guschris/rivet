mod exec;
mod leader;
mod reconciler;
mod spec;
mod state;

use clap::Parser;
use sched_lib::Scheduler;
use std::collections::BTreeMap;
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

    #[arg(long, help = "Compute plan and output JSON to stdout, then exit (no execution)")]
    plan_only: bool,

    #[arg(long, value_name = "FILE", help = "Read plan JSON from file and execute it")]
    plan_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let scheduler = Scheduler::from_str(&cli.scheduler).unwrap_or_else(|e| {
        eprintln!("flockd: {}", e);
        std::process::exit(1);
    });

    if let Some(ref plan_path) = cli.plan_file {
        run_plan_file(plan_path, &cli.state);
        return;
    }

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

    if cli.plan_only {
        run_plan_once(
            &db,
            &cli.specs,
            &cli.exec_create,
            &cli.exec_delete,
            &cli.exec_health,
            &scheduler,
            &cli.nodes_file,
            &cli.node_health_cmd,
        );
        if let Some(f) = lock_guard.take() {
            leader::release(f);
        }
        return;
    }

    let mut ticker = time::interval(Duration::from_secs(cli.interval));
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    let mut last_spec_hashes: BTreeMap<String, String> = BTreeMap::new();
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
            let hash = match spec::hash_spec(s) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("flockd: hash error for '{}': {}", name, e);
                    continue;
                }
            };
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
            true,
        );

        for action in &actions {
            eprintln!("flockd: {}", action.message());
        }

        if actions.is_empty() && !changed {
            continue;
        }
    }

    if let Some(f) = lock_guard.take() {
        leader::release(f);
    }
    eprintln!("flockd: reconciler stopped");
}

fn load_nodes(nodes_file: &Option<PathBuf>) -> Vec<String> {
    match nodes_file {
        Some(path) => {
            let content = std::fs::read_to_string(path).unwrap_or_default();
            content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect()
        }
        None => vec!["localhost".into()],
    }
}

fn run_plan_once(
    db: &state::StateDB,
    specs_path: &PathBuf,
    exec_create: &str,
    exec_delete: &str,
    exec_health: &str,
    scheduler: &Scheduler,
    nodes_file: &Option<PathBuf>,
    node_health_cmd: &str,
) {
    let specs = spec::load_specs(specs_path).unwrap_or_else(|e| {
        eprintln!("flockd: error loading specs: {}", e);
        std::process::exit(1);
    });

    let nodes = load_nodes(nodes_file);

    if !node_health_cmd.is_empty() {
        let actions = reconciler::check_node_health(db, &nodes, node_health_cmd);
        for action in &actions {
            eprintln!("flockd: {}", action);
        }
    }

    db.begin_transaction().unwrap_or_else(|e| {
        eprintln!("flockd: transaction error: {}", e);
        std::process::exit(1);
    });

    let plan = reconciler::reconcile(
        db, &specs, &nodes, scheduler, exec_create, exec_delete, exec_health, false,
    );

    let json = serde_json::to_string_pretty(&plan).unwrap_or_else(|e| {
        eprintln!("flockd: JSON serialization error: {}", e);
        "[]".into()
    });
    println!("{}", json);

    db.rollback_transaction().unwrap_or_else(|e| {
        eprintln!("flockd: rollback error: {}", e);
    });
}

fn run_plan_file(plan_path: &PathBuf, state_path: &PathBuf) {
    let content = std::fs::read_to_string(plan_path).unwrap_or_else(|e| {
        eprintln!("flockd: error reading plan file: {}", e);
        std::process::exit(1);
    });

    let plan: Vec<reconciler::PlanAction> = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("flockd: invalid plan JSON: {}", e);
        std::process::exit(1);
    });

    let db = state::StateDB::open(state_path).unwrap_or_else(|e| {
        eprintln!("flockd: {}", e);
        std::process::exit(1);
    });

    for action in &plan {
        eprintln!("flockd: {}", action.message());
        match reconciler::execute_plan_action(&db, action) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("flockd: action error: {}", e);
            }
        }
    }
}
