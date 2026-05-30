# probe — Async Health Checker

A standalone health-checker that reads a JSON file of target definitions and streams NDJSON health status to stdout. Runs TCP connect, HTTP GET, or shell command checks concurrently. Useful as a monitoring sidecar, circuit-breaker feed, or uptime monitor — completely decoupled from workload supervision.

## Quick Start

```bash
# Create targets file
cat > targets.json <<'EOF'
[
  {"name": "web",   "type": "tcp",  "host": "127.0.0.1", "port": 8080},
  {"name": "api",   "type": "http", "host": "127.0.0.1", "port": 3000, "path": "/health"},
  {"name": "cron",  "type": "exec", "command": "pgrep myapp"}
]
EOF

# Run one check cycle
probe --targets targets.json --once

# Run continuously every 5 seconds
probe --targets targets.json

# Text output format
probe --targets targets.json --once --format text
```

## CLI Reference

```
probe --targets <FILE> [--interval 5] [--format json|text] [--once]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--targets` | JSON file with target definitions | *required* |
| `--interval` | Check interval in seconds | `5` |
| `--format` | Output format: `json` (NDJSON) or `text` | `json` |
| `--once` | Run one check cycle and exit | *(loop forever)* |

## Targets File Format

A JSON array of target objects:

```json
[
  {
    "name": "my-service",
    "type": "tcp",
    "host": "127.0.0.1",
    "port": 8080
  },
  {
    "name": "my-api",
    "type": "http",
    "host": "127.0.0.1",
    "port": 3000,
    "path": "/health"
  },
  {
    "name": "my-daemon",
    "type": "exec",
    "command": "systemctl is-active my-daemon"
  }
]
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Target identifier |
| `type` | string | yes | `tcp`, `http`, or `exec` |
| `host` | string | TCP/HTTP | Hostname or IP (default: `127.0.0.1`) |
| `port` | number | TCP/HTTP | TCP port |
| `path` | string | HTTP | Request path (default: `/`) |
| `command` | string | exec | Shell command (run via `sh -c`) |

## JSON Output (NDJSON)

One JSON object per target per cycle, written to stdout:

```json
{"target":"web","type":"tcp","healthy":true,"elapsed_ms":2,"ts_ms":1717000000000}
{"target":"api","type":"http","healthy":false,"elapsed_ms":2034,"ts_ms":1717000000001}
```

- `healthy` — `true` if the check passed.
- `elapsed_ms` — wall-clock time for the check.
- `ts_ms` — Unix epoch milliseconds when the check completed.

Pipe to `jq` for filtering: `probe --targets t.json --once | jq 'select(.healthy == false)'`

## Text Output

```
web UP 2ms
api DOWN 2034ms
cron UP 15ms
```

## Standalone Use Cases

- **Circuit breaker sidecar:** pipe probe output to an Envoy/HAProxy health endpoint.
- **Uptime monitoring:** redirect to a log file, tail with alerting scripts.
- **Pre-flight checks:** `probe --targets checks.json --once` in CI pipelines.

## See Also

- [Quickstart](quickstart.md) — 5-minute hands-on walkthrough
- [podlet](podlet.md) — Workload supervisor with its own embedded health checks
- [flockd](flockd.md) — Declarative reconciler that uses health status for orchestration
- [Integration Guide](integration.md) — How probe composes with the rest of the toolkit
