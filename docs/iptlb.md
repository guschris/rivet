# iptlb — Dummy L4 Load Balancer

A tiny file-driven TCP load balancer that reads a plaintext list of backends and updates kernel iptables DNAT rules. When the backends file changes, rules are reapplied. Existing connections are preserved.

## Quick Start

```bash
# Create a backends file
echo "10.0.0.2:8080" > /mnt/state/services/web.backend
echo "10.0.0.3:8080" >> /mnt/state/services/web.backend

# Start the load balancer
iptlb --vip 10.0.0.100 --port 80 --backends-file /mnt/state/services/web.backend

# Traffic to 10.0.0.100:80 is now round-robin DNAT'd to the backends
```

## CLI Reference

```
iptlb --vip <ip>
      --port <port>
      --backends-file <path>
      [--scheduler rr]
      [--interval 2]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--vip` | Virtual IP address to DNAT from | *required* |
| `--port` | TCP port to match | *required* |
| `--backends-file` | Path to plaintext backend list | *required* |
| `--scheduler` | `rr` (round-robin) | `rr` |
| `--interval` | Poll interval in seconds | `2` |

## Backends File Format

One backend per line, format `IP:PORT`. Lines starting with `#` are comments. Blank lines are ignored. If no port is given, `:80` is assumed.

```
# Web servers
10.0.0.2:8080
10.0.0.3:8080
10.0.0.4          # defaults to :80
```

## How It Works

1. Polls the backends file every `--interval` seconds.
2. Detects changes by hashing the backend list.
3. Creates a custom iptables chain `IPTLB-<port>-<vip-hyphenated>` in the `nat` table.
4. Adds a jump rule in `PREROUTING`: traffic to `--vip:--port` jumps to the custom chain.
5. Populates the chain with `statistic --mode nth` rules for round-robin DNAT.
6. On change, flushes and repopulates the chain.

Requires **root** (or `CAP_NET_ADMIN`) and the `iptables` binary.

## Example: Dynamic Backends

Write to the backends file from any process — `iptlb` picks up changes within `--interval` seconds:

```bash
# Add a backend
echo "10.0.0.5:8080" >> /mnt/state/services/web.backend

# Remove all backends and replace
echo "10.0.0.6:8080" > /mnt/state/services/web.backend
```

## See Also

- [podlet](podlet.md) — Workload supervisor that produces backend endpoints
- [flockd](flockd.md) — Declarative reconciler that can write the backends file automatically
- [Integration Guide](integration.md) — How the three tools compose into a full orchestrator
