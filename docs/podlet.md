# podlet — Workload Supervisor

Supervises a single long-lived unit: a process, a [Podman](https://podman.io) container, or a KVM virtual machine. Provides health checks, resource limits, restart policies, and graceful shutdown.

## Quick Start

```bash
# Supervise a simple process
podlet --name myapp --tcp-check :8080 -- /usr/bin/myapp --port 8080

# Supervise a container
podlet --name web --type container --ports 80:8080 -- docker.io/nginx:alpine

# With restart policy
podlet --name api --restart always --max-restarts 5 -- ./api-server
```

## CLI Reference

```
podlet --name <name>
      [--type process|container|vm]
      [--cpu <limit>]
      [--mem <limit>]
      [--ports <container>:<host>]...
      [--tcp-check :port]
      [--http-check /path]
      [--exec-check "cmd"]
      [--restart always|on-failure]
      [--max-restarts N]
      [--drain-timeout 30s]
      [--health-interval 5s]
      -- <command>...
```

| Flag | Description | Default |
|------|-------------|---------|
| `--name` | Unique name for this workload | *required* |
| `--type` | `process`, `container`, or `vm` | `process` |
| `--cpu` | CPU limit as fractional cores (cgroups v2) | none |
| `--mem` | Memory limit (e.g. `64Mi`, `1G`) | none |
| `--ports` | Container-to-host port mapping, repeatable | none |
| `--tcp-check` | Health check: TCP connect to `:port` | none |
| `--http-check` | Health check: HTTP GET `:port/path` | none |
| `--exec-check` | Health check: run shell command | none |
| `--restart` | `always` or `on-failure` | `never` |
| `--max-restarts` | Max restart attempts before giving up | 10 |
| `--drain-timeout` | Wait after SIGTERM before SIGKILL | `30s` |
| `--health-interval` | Seconds between health checks | `5s` |

## JSON Output

`podlet` writes one JSON object per line to stdout, describing the current state:

```json
{"status":"starting","pid":null,"health":"unknown","ports":{}}
{"status":"running","pid":1234,"health":"healthy","ports":{"80":"32771"}}
{"status":"exited","pid":null,"health":"unknown","ports":{}}
```

Status values: `starting`, `running`, `draining`, `exited`, `stopped`.
Health values: `unknown`, `healthy`, `unhealthy`.

## Health Checks

Exactly one health check type can be specified:

- **TCP**: `--tcp-check :8080` — connects to `127.0.0.1:8080`, healthy if connection succeeds.
- **HTTP**: `--http-check :8080/health` — sends `GET /health HTTP/1.0`, healthy if status is 2xx or 3xx.
- **Exec**: `--exec-check "pgrep myapp"` — runs the command via `sh -c`, healthy if exit code 0.

## Signal Handling

`podlet` handles **SIGINT** and **SIGTERM**:

1. Forwards SIGTERM to the child process.
2. Waits `--drain-timeout` for the child to exit.
3. If still alive, sends SIGKILL.

## Resource Limits (cgroups v2)

When `--cpu` or `--mem` is specified, `podlet` creates a cgroup `/sys/fs/cgroup/<name>/` and assigns the child PID. Requires **root**.

- CPU: writes to `cpu.max` as `"<quota> <period>"` (period = 100000µs).
- Memory: writes to `memory.max`.

## Systemd Integration

For production use, wrap `podlet` with systemd for auto-restart on crash:

```ini
# /etc/systemd/system/podlet@.service
[Service]
ExecStart=/usr/local/bin/podlet --name %i --restart always -- %i
Restart=always
```

Start with: `systemctl start podlet@myapp`

## See Also

- [iptlb](iptlb.md) — L4 load balancer that can route traffic to podlet-managed backends
- [flockd](flockd.md) — Declarative reconciler that orchestrates podlet across nodes
- [Integration Guide](integration.md) — How the three tools compose into a full orchestrator
