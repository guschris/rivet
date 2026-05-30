# Hand-over Document: microk8s-like Unix Toolkit (Rust)

## 1. Introduction & Motivation

The goal is to provide a **Kubernetes-like orchestration experience** without the overwhelming complexity, size, and operational burden of Kubernetes itself. We want a system that:

- Manages **processes, Podman containers, and KVM VMs** equally.
- Is **100× smaller** than Kubernetes (<2500 lines of Rust total).
- Uses **composable Unix-style utilities** that can be understood, debugged, and reused independently.
- Avoids heavy dependencies (etcd, API server, controllers) and fragile patterns (inotify on NFS).
- Works on a handful of nodes, with NFSv4 as the sole shared state.

We observed that Kubernetes’ core value is **declarative state reconciliation + health-checked supervision + service discovery**, and that these can be implemented with tiny, sharp tools that fit in your head.

---

## 2. High-Level Architecture

The system consists of **three standalone CLI tools** (each useful on its own) that, when combined with a few lines of shell glue and an NFSv4 mount, form a complete orchestrator.

- **`podlet`** – Workload supervisor (process/container/VM) with health checks, resource limits, and graceful shutdown.
- **`flockd`** – Declarative reconciler that turns YAML spec files into reality by executing user‑provided commands on a cluster.
- **`iptlb`** – File‑driven L4 load balancer that updates kernel iptables rules when a backends file changes.

All coordination happens through **plain files on an NFSv4 shared volume**, eliminating the need for a database or consensus algorithm. Polling is used instead of `inotify` to ensure reliability across network filesystems.

---

## 3. Core Utilities

### 3.1 `podlet` – The Workload Supervisor

**Purpose:** Run and supervise a single long‑lived unit (process, container, VM). It’s a self‑contained replacement for a systemd unit that also does health checks.

**Interface:**
```
podlet --name <name> [--type process|container|vm] [--cpu <limit>] [--mem <limit>] \
       [--ports <container>:<host>] [--tcp-check :port] [--http-check /path] \
       [--exec-check "cmd"] [--restart always|on-failure] [--max-restarts N] \
       [--drain-timeout 30s] -- <command>   # for processes
```

**Key behaviors:**
- Spawns the workload, applies cgroups for resource limits.
- For containers: wraps `podman run`. For VMs: wraps `qemu-system-x86_64`.
- Port forwarding: allocates a random host port if none specified, writes mapping to stdout.
- Outputs JSON lines to stdout describing state: `{"status":"running","pid":1234,"health":"healthy","ports":{"80":"32771"}}`.
- On SIGTERM, forwards it to workload and waits for drain timeout before SIGKILL.
- **Independent use:** can be run manually, piped to `jq`, or wrapped by systemd (which we recommend for auto-restart in production).

**Design decision:** No heartbeat file. `podlet` is purely about local supervision. The orchestration layer will monitor it externally.

### 3.2 `flockd` – The Declarative Reconciler

**Purpose:** A generic reconciler that reads YAML/JSON spec files, compares them to a state database, and executes commands to converge reality. It is **not** Kubernetes‑specific; it’s a tiny "infrastructure reconciler" engine.

**Interface:**
```
flockd --specs /path/to/specs.d --state /path/to/state.db \
       --exec-create "ssh {node} systemctl start podlet@{name}" \
       --exec-delete "ssh {node} systemctl stop podlet@{name}" \
       --exec-health "cat /mnt/state/heartbeats/{name}" \
       --scheduler first-fit --nodes-file /etc/flock/nodes
```

**Key behaviors:**
- Polls the specs directory every 5 seconds (no inotify). Hashes specs to detect changes.
- Maintains a SQLite state database of desired vs. actual state (tracked by the tool itself, not fetched automatically – the user must provide the health check command).
- For each spec, if the number of healthy replicas < desired, it runs the `--exec-create` command after scheduling (picking a node from `--nodes-file` using a simple first‑fit or best‑fit algorithm).
- If too many replicas, it runs `--exec-delete` on extra ones (preferring unhealthy first).
- Supports rolling update strategies: when a spec changes, it adds new replicas, waits for health, then drains and deletes old ones (using a configurable drain timeout from the spec).
- **Node failure detection:** not built‑in. Instead, we feed it node liveness via an external script that updates the state DB or heartbeats. `flockd` can also accept a `--node-health-cmd` that returns 0/1 for a node, used to mark nodes down and reschedule their workloads.
- **Independent use:** Can be used to maintain DNS records, cloud VMs, or anything that can be expressed as desired state + commands.

**Design decision:** All cluster logic is driven by `flockd`. The scheduler is just a function that maps workloads to nodes; the exec commands are user‑supplied (e.g., `ssh … systemctl …`), so `flockd` stays completely agnostic about transport and workload type.

### 3.3 `iptlb` – The Dummy Load Balancer

**Purpose:** A tiny L4 load balancer that reads a plaintext file of backends and updates iptables DNAT rules (or IPVS) atomically.

**Interface:**
```
iptlb --vip <ip> --port <port> --backends-file /path/to/backends.txt [--scheduler lc] [--interval 2]
```

**Key behaviors:**
- Polls the backends file every `--interval` seconds (default 2).
- When the file changes, atomically swaps iptables rules so there’s no disruption to existing connections.
- Round‑robin or least‑connection scheduling.
- **Independent use:** Perfect for simple TCP load balancing without any orchestration; just pipe a list of IP:port into the file.

---

## 4. Shared Infrastructure & Coordination

All three tools (plus any glue scripts) share a single NFSv4 mount at `/mnt/state/` with this layout:

```
/mnt/state/
├── specs/               # Git repo of desired state (plain YAML, no templates)
│   ├── prod/frontend.yaml
│   └── staging/...
├── heartbeats/          # touch‑based liveness files per podlet instance
├── state.db             # flockd's SQLite database (can be on NFSv4)
├── flockd.lock          # flock-based lock for leader election
└── services/            # runtime backend lists for iptlb
    └── frontend.backend
```

**NFSv4 reliability:** We rely on `flock()` lock leasing (NFSv4) for `flockd` master election. The lock is automatically released if the master dies, allowing a backup to take over within seconds. We **avoid `inotify` entirely** and use polling everywhere—this is a deliberate design choice for NFS.

**Leader election:** A simple wrapper script uses `flock -n /mnt/state/flockd.lock -c 'flockd ...'` to ensure only one active `flockd`. Backups try every 3 seconds.

---

## 5. Handling Failure & Upgrades

### 5.1 Node Failure

- Each worker node runs a tiny cron script that updates `/mnt/state/heartbeats/<hostname>` with a timestamp (or the podlet heartbeat wrapper writes per‑podlet health).
- `flockd` checks these heartbeat files (via `--node-health-cmd` or direct file stat). If a heartbeat is older than 10 seconds, the node is marked DOWN.
- All workloads on that node are immediately scheduled to other nodes via `--exec-create`.
- When the node returns, `flockd` detects the extra replicas and scales down gracefully.

### 5.2 Podlet Failure (on alive machine)

- We rely on **systemd** to restart `podlet` locally. The `podlet@.service` unit has `Restart=always`.
- Heartbeat tolerance in `flockd` (>30 seconds) allows for brief restarts without triggering cluster‑wide rescheduling.
- If the podlet can't recover, the heartbeat stays dead, and `flockd` will clean up the systemd unit and reschedule elsewhere.

### 5.3 Rolling Upgrades

- User changes a spec file (new image tag). `flockd` detects the spec hash change.
- For each replica, it starts a new podlet, waits for health, adds to backend list, then drains and stops the old podlet.
- The load balancer (`iptlb`) sees the backend list update and smoothly shifts traffic. No dropped connections if the application handles SIGTERM gracefully.

---

## 6. Job Queue Extension

We added a lightweight **batch job system** that reuses the same infrastructure.

- Spec file kind: `JobQueue` – defines a concurrency level (number of worker podlets), container image, and retry policy.
- `flockd` ensures the correct number of worker podlets are running, just like a service.
- Workers coordinate through a **job directory** on NFS:
  - `incoming/` – new job JSON files.
  - `active/` – workers atomically rename jobs here to claim them.
  - `done/` and `failed/` for results.
  - `retry/` – `flockd` periodically moves retry‑able jobs back to `incoming`.
- A tiny `submit-job` CLI writes JSON files to `incoming/`.

This requires no message broker, just files and atomic renames.

---

## 7. Deployment & GitOps

We completely avoid Helm and templated YAML.

- The `specs/` directory is a **Git repository**. Branch = environment.
- A simple `git-sync` timer on the `flockd` host runs `git pull` periodically.
- Specs are plain, fully resolved YAML. For code reuse, we provide an optional `specmerge` tool (100 lines) that deep‑merges a base spec with environment patches (like Kustomize but minimal).
- No `helm install`, no `kubectl apply`, no charts. Just Git + `flockd`.

Secrets are handled outside Git: `podlet` can accept environment variables or a `--env-file`, and a separate `secret` helper (e.g., from `pass`) injects them at start time.

---

## 8. Implementation Plan (Rust)

We’ll implement three (plus optional) binaries in a single Rust workspace:

- **`podlet`** – ~800 lines. Uses `nix` crate for cgroups, `tokio` for async process management, health checks.
- **`flockd`** – ~1200 lines. Uses `serde_yaml`, `rusqlite` for state, `ssh2` crate or `Command` for exec. Scheduler is a simple module. Rolling update logic.
- **`iptlb`** – ~300 lines. Uses `iptables` crate or raw `iptables-restore`. Polling via `tokio::time::interval`.
- **`specmerge`** (optional) – ~100 lines.
- **`submit-job`** – ~80 lines.

Total: <2500 lines of Rust, compilable to static binaries. No external runtime dependencies beyond the Linux kernel and NFSv4 mount.

**Error handling:** All tools will return proper exit codes and use structured logging. `flockd` will log actions to stdout/JSON for monitoring.

---

## 9. Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| NFSv4 as sole coordination store | Avoids etcd/consul; simple files are debuggable; `flock` gives us HA leader election. |
| Polling instead of `inotify` | NFS does not reliably support `inotify` for cross‑client events; polling is predictable. |
| systemd for podlet supervision | Provides mature local restart, dependency handling; no need to reinvent init. |
| Plain YAML specs, no templates | Simplicity; GitOps; no string‑interpolation hell. `specmerge` covers common config reuse. |
| Health checks inside podlet, not external | The supervisor knows its workload best; reduces network coordination. |
| Exec‑based orchestration in `flockd` | Keeps the tool generic; user provides the actual deploy/stop commands (e.g., `ssh`, `systemctl`). |
| Atomic file renames for job claiming | Simple, NFS‑safe (NFSv4 guarantees atomic rename), avoids lock files. |
| Rust implementation | Memory safety, static binaries, low resource usage, community appeal. |

---

## 10. Next Steps

1. **Implement `podlet`** as a standalone MVP: process and container support with health checks and JSON output.
2. **Implement `iptlb`** – simple backend file polling and iptables management.
3. **Implement `flockd`** with a basic reconciler, single node first, then add scheduler and HA lock.
4. **Create glue scripts** (`podlet-heartbeat`, `git-sync`, `flockd`‑wrapper) and systemd unit files.
5. **End‑to‑end test** on two VMs with NFSv4, deploying a web app with rolling update.
6. **Add job queue** extension.

The beauty is that each step produces a useful tool, and they can be composed later. You can start using `podlet` right away for local development without any orchestration.

---

**Passing the baton.** This document captures the spirit of the design: tiny, composable, and Unix‑philosophy‑driven. Feel free to challenge any decision, but the core invariant is to keep each tool independently useful and the whole system under a few thousand lines of Rust. Good luck!