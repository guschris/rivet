use tokio::process::Command;

use crate::PortMapping;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    Never,
    Always,
    OnFailure,
}

pub fn spawn(
    type_: &str,
    command: &[String],
    ports: &[(PortMapping, u16)],
    name: &str,
) -> Result<tokio::process::Child, String> {
    match type_ {
        "process" => spawn_process(command),
        "container" => spawn_container(command, ports, name),
        "vm" => spawn_vm(command),
        _ => Err(format!("unknown workload type: {}", type_)),
    }
}

fn spawn_process(command: &[String]) -> Result<tokio::process::Child, String> {
    if command.is_empty() {
        return Err("no command provided".into());
    }
    let prog = &command[0];
    let args = &command[1..];

    Command::new(prog)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn '{}': {}", prog, e))
}

fn spawn_container(
    command: &[String],
    ports: &[(PortMapping, u16)],
    name: &str,
) -> Result<tokio::process::Child, String> {
    if command.is_empty() {
        return Err("no container image specified".into());
    }

    let mut cmd = Command::new("podman");
    cmd.arg("run")
        .arg("--name")
        .arg(name)
        .arg("--rm")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null());

    for (mapping, host_port) in ports {
        cmd.arg("-p")
            .arg(format!("{}:{}", host_port, mapping.container_port));
    }

    for arg in command {
        cmd.arg(arg);
    }

    cmd.spawn()
        .map_err(|e| format!("failed to run podman: {}", e))
}

fn spawn_vm(command: &[String]) -> Result<tokio::process::Child, String> {
    if command.is_empty() {
        return Err("no qemu arguments provided".into());
    }

    Command::new("qemu-system-x86_64")
        .args(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to start qemu: {}", e))
}
