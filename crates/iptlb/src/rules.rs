use std::net::SocketAddr;
use std::process::Command;

pub fn chain_name(vip: &str) -> String {
    format!("IPTLB-{}", vip.replace(['.', ':'], "-"))
}

pub fn build_rules(
    vip: &str,
    port: u16,
    chain_name: &str,
    backends: &[SocketAddr],
) -> Result<Vec<Vec<String>>, String> {
    if backends.is_empty() {
        return Ok(vec![]);
    }

    let mut cmds = Vec::new();
    let n = backends.len();

    for (i, backend) in backends.iter().enumerate() {
        let cmd = vec![
            "-t".to_string(),
            "nat".to_string(),
            "-A".to_string(),
            chain_name.to_string(),
            "-d".to_string(),
            vip.to_string(),
            "-p".to_string(),
            "tcp".to_string(),
            "--dport".to_string(),
            port.to_string(),
            "-m".to_string(),
            "statistic".to_string(),
            "--mode".to_string(),
            "nth".to_string(),
            "--every".to_string(),
            n.to_string(),
            "--packet".to_string(),
            i.to_string(),
            "-j".to_string(),
            "DNAT".to_string(),
            "--to-destination".to_string(),
            backend.to_string(),
        ];
        cmds.push(cmd);
    }

    Ok(cmds)
}

pub struct RuleManager;

impl RuleManager {
    pub fn new() -> Self {
        RuleManager
    }

    pub fn ensure_chain(&self, chain_name: &str) -> Result<(), String> {
        let output = Command::new("iptables")
            .args(["-t", "nat", "-L", chain_name])
            .output()
            .map_err(|e| format!("cannot run iptables: {}", e))?;

        if output.status.success() {
            return Ok(());
        }

        let status = Command::new("iptables")
            .args(["-t", "nat", "-N", chain_name])
            .status()
            .map_err(|e| format!("cannot run iptables: {}", e))?;

        if !status.success() {
            return Err(format!("iptables -t nat -N {} failed", chain_name));
        }

        Ok(())
    }

    pub fn ensure_jump_rule(&self, vip: &str, port: u16, chain_name: &str) -> Result<(), String> {
        let output = Command::new("iptables")
            .args([
                "-t", "nat", "-C", "PREROUTING",
                "-d", vip, "-p", "tcp", "--dport", &port.to_string(),
                "-j", chain_name,
            ])
            .output()
            .map_err(|e| format!("cannot run iptables: {}", e))?;

        if output.status.success() {
            return Ok(());
        }

        let status = Command::new("iptables")
            .args([
                "-t", "nat", "-A", "PREROUTING",
                "-d", vip, "-p", "tcp", "--dport", &port.to_string(),
                "-j", chain_name,
            ])
            .status()
            .map_err(|e| format!("cannot run iptables: {}", e))?;

        if !status.success() {
            return Err("failed to add jump rule to PREROUTING".into());
        }

        Ok(())
    }

    pub fn apply_rules(&self, chain_name: &str, cmds: &[Vec<String>]) -> Result<(), String> {
        let flush_status = Command::new("iptables")
            .args(["-t", "nat", "-F", chain_name])
            .status()
            .map_err(|e| format!("cannot flush chain: {}", e))?;

        if !flush_status.success() {
            return Err(format!("failed to flush chain {}", chain_name));
        }

        for cmd in cmds {
            let status = Command::new("iptables")
                .args(cmd)
                .status()
                .map_err(|e| format!("cannot add DNAT rule: {}", e))?;

            if !status.success() {
                return Err(format!("failed to add rule: iptables {}", cmd.join(" ")));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn chain_name_replaces_dots_and_colons() {
        assert_eq!(chain_name("10.0.0.1"), "IPTLB-10-0-0-1");
        assert_eq!(chain_name("192.168.1.1"), "IPTLB-192-168-1-1");
    }

    #[test]
    fn empty_backends_produces_no_rules() {
        let rules = build_rules("10.0.0.1", 80, "TEST", &[]).unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn single_backend_generates_one_rule() {
        let backends: Vec<SocketAddr> = vec!["10.0.0.2:8080".parse().unwrap()];
        let rules = build_rules("10.0.0.1", 80, "TEST", &backends).unwrap();
        assert_eq!(rules.len(), 1);

        let cmd = rules[0].join(" ");
        assert!(cmd.contains("-j DNAT"));
        assert!(cmd.contains("10.0.0.2:8080"));
        assert!(cmd.contains("--every 1"));
        assert!(cmd.contains("--packet 0"));
    }

    #[test]
    fn multiple_backends_generate_correct_nth_rules() {
        let backends: Vec<SocketAddr> = vec![
            "10.0.0.2:8080".parse().unwrap(),
            "10.0.0.3:8080".parse().unwrap(),
            "10.0.0.4:8080".parse().unwrap(),
        ];
        let rules = build_rules("10.0.0.1", 80, "TEST", &backends).unwrap();
        assert_eq!(rules.len(), 3);

        let cmd0 = rules[0].join(" ");
        assert!(cmd0.contains("--every 3"));
        assert!(cmd0.contains("--packet 0"));
        assert!(cmd0.contains("10.0.0.2:8080"));

        let cmd1 = rules[1].join(" ");
        assert!(cmd1.contains("--every 3"));
        assert!(cmd1.contains("--packet 1"));
        assert!(cmd1.contains("10.0.0.3:8080"));

        let cmd2 = rules[2].join(" ");
        assert!(cmd2.contains("--every 3"));
        assert!(cmd2.contains("--packet 2"));
        assert!(cmd2.contains("10.0.0.4:8080"));
    }
}
