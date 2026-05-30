use clap::Parser;
use sched_lib::{schedule, Scheduler};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "sched",
    about = "Bin-packing scheduler; maps workloads to nodes"
)]
struct Cli {
    #[arg(
        long,
        default_value = "first-fit",
        help = "Scheduling strategy: first-fit or best-fit"
    )]
    strategy: String,

    #[arg(long, value_name = "FILE", help = "JSON input file (reads from stdin if omitted)")]
    input: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct Input {
    nodes: Vec<String>,
    #[serde(default)]
    loads: HashMap<String, u32>,
    spec_name: String,
    #[serde(default)]
    next_index: u32,
}

fn read_input(cli: &Cli) -> Result<Input, String> {
    let json_str = if let Some(ref path) = cli.input {
        std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("stdin: {}", e))?;
        buf
    };

    serde_json::from_str(&json_str)
        .map_err(|e| format!("invalid JSON input: {}", e))
}

fn main() {
    let cli = Cli::parse();

    let strategy = Scheduler::from_str(&cli.strategy).unwrap_or_else(|e| {
        eprintln!("sched: {}", e);
        std::process::exit(1);
    });

    let input = read_input(&cli).unwrap_or_else(|e| {
        eprintln!("sched: error: {}", e);
        std::process::exit(1);
    });

    let node_loads: Vec<(String, u32)> = input
        .nodes
        .iter()
        .map(|n| {
            let count = input.loads.get(n).copied().unwrap_or(0);
            (n.clone(), count)
        })
        .collect();

    match schedule(&strategy, &input.nodes, &node_loads, &input.spec_name, input.next_index) {
        Some(result) => {
            let output =
                serde_json::to_string(&result).unwrap_or_else(|_| "{}".into());
            println!("{}", output);
        }
        None => {
            eprintln!("sched: no available nodes");
            std::process::exit(1);
        }
    }
}
