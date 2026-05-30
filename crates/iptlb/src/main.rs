use clap::Parser;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time;

mod backend;
mod rules;

#[derive(Parser, Debug)]
#[command(name = "iptlb", about = "File-driven L4 load balancer using iptables DNAT")]
pub struct Cli {
    #[arg(long)]
    pub vip: String,

    #[arg(long)]
    pub port: u16,

    #[arg(long)]
    pub backends_file: PathBuf,

    #[arg(long, default_value = "rr")]
    pub scheduler: String,

    #[arg(long, default_value = "2")]
    pub interval: u64,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.scheduler != "rr" {
        eprintln!(
            "iptlb: only 'rr' (round-robin) scheduler is supported, got '{}'",
            cli.scheduler
        );
        std::process::exit(1);
    }

    let chain_name = rules::chain_name(&cli.vip);
    let manager = rules::RuleManager::new();

    manager.ensure_chain(&chain_name).unwrap_or_else(|e| {
        eprintln!("iptlb: warning: chain setup failed: {}", e);
    });

    manager
        .ensure_jump_rule(&cli.vip, cli.port, &chain_name)
        .unwrap_or_else(|e| {
            eprintln!("iptlb: warning: jump rule setup failed: {}", e);
        });

    let mut last_hash = String::new();
    let mut ticker = time::interval(Duration::from_secs(cli.interval));

    loop {
        ticker.tick().await;

        let backends = match backend::parse_file(&cli.backends_file) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("iptlb: {}", e);
                continue;
            }
        };

        let current_hash = hash_backends(&backends);
        if current_hash == last_hash {
            continue;
        }
        last_hash = current_hash;

        eprintln!(
            "iptlb: backends changed, updating rules ({} backends)",
            backends.len()
        );

        match rules::build_rules(&cli.vip, cli.port, &chain_name, &backends) {
            Ok(cmds) => {
                if let Err(e) = manager.apply_rules(&chain_name, &cmds) {
                    eprintln!("iptlb: failed to apply rules: {}", e);
                } else {
                    eprintln!("iptlb: rules updated successfully");
                }
            }
            Err(e) => eprintln!("iptlb: {}", e),
        }
    }
}

pub fn hash_backends(backends: &[SocketAddr]) -> String {
    let mut hasher = DefaultHasher::new();
    for b in backends {
        b.to_string().hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}
