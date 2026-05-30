use std::process::Command;

pub fn substitute(template: &str, name: &str, node: &str) -> String {
    template
        .replace("{name}", name)
        .replace("{node}", node)
}

pub fn run_command(command: &str) -> Result<bool, String> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| format!("cannot execute command: {}", e))?;

    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_placeholders() {
        let result = substitute("ssh {node} start {name}", "frontend", "node1");
        assert_eq!(result, "ssh node1 start frontend");
    }

    #[test]
    fn handles_multiple_placeholders() {
        let result = substitute("{node}:{name}:{node}", "app", "host1");
        assert_eq!(result, "host1:app:host1");
    }
}
