use std::fs;
use std::path::PathBuf;

fn cgroup_path(name: &str) -> PathBuf {
    PathBuf::from("/sys/fs/cgroup").join(name)
}

pub fn apply_cpu_limit(name: &str, cpu: f64) -> Result<(), String> {
    let dir = cgroup_path(name);
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create cgroup dir: {}", e))?;

    let period = 100_000u64;
    let quota = (cpu * period as f64) as u64;
    let content = if quota == 0 {
        "max 100000".into()
    } else {
        format!("{} {}", quota, period)
    };

    let cpu_max = dir.join("cpu.max");
    fs::write(&cpu_max, &content)
        .map_err(|e| format!("failed to write cpu.max: {}", e))?;

    Ok(())
}

pub fn apply_mem_limit(name: &str, bytes: u64) -> Result<(), String> {
    let dir = cgroup_path(name);
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create cgroup dir: {}", e))?;

    let mem_max = dir.join("memory.max");
    fs::write(&mem_max, bytes.to_string())
        .map_err(|e| format!("failed to write memory.max: {}", e))?;

    Ok(())
}

pub fn assign_pid(name: &str, pid: u32) -> Result<(), String> {
    let dir = cgroup_path(name);
    let procs = dir.join("cgroup.procs");
    fs::write(&procs, pid.to_string())
        .map_err(|e| format!("failed to assign pid to cgroup: {}", e))?;

    Ok(())
}

pub fn cleanup_cgroup(name: &str) {
    let dir = cgroup_path(name);
    let _ = fs::remove_dir(&dir);
}
