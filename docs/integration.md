# Integration Guide — not-k8s Full Orchestrator

This guide shows how [podlet](podlet.md), [iptlb](iptlb.md), and [flockd](flockd.md) compose into a complete Kubernetes-like orchestrator — without Kubernetes.

## The Toolkit

| Tool | Role | Docs |
|------|------|------|
| **[podlet](podlet.md)** | Workload supervisor (process/container/VM) | [podlet.md](podlet.md) |
| **[flockd](flockd.md)** | Declarative reconciler + scheduler | [flockd.md](flockd.md) |
| **[iptlb](iptlb.md)** | L4 TCP load balancer (iptables DNAT) | [iptlb.md](iptlb.md) |
| **[merge](merge.md)** | Deep-merge YAML/JSON configs (RFC 7396) | [merge.md](merge.md) |
| **[probe](probe.md)** | Async health-checker (TCP/HTTP/exec), NDJSON output | [probe.md](probe.md) |
| **[sched](sched.md)** | Standalone bin-packing scheduler (first-fit/best-fit) | [sched.md](sched.md) |

The three core tools (podlet, flockd, iptlb) form the orchestrator. The three support tools (merge, probe, sched) are individually useful and can be adopted independently.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                   /mnt/state (NFSv4)                  │
│  ┌──────────┐  ┌───────────┐  ┌───────────────────┐  │
│  │ specs/   │  │heartbeats/│  │services/           │  │
│  │ *.yaml   │  │  <host>   │  │ frontend.backend   │  │
│  └────┬─────┘  └─────┬─────┘  └────────┬──────────┘  │
│       │              │                 │              │
│  ┌────┴──────┐  ┌────┴──────┐  ┌───────┴──────────┐  │
│  │  flockd   │  │  podlet   │  │     iptlb        │  │
│  │ reconciler│  │supervisor │  │  load balancer   │  │
│  └────┬──────┘  └────┬──────┘  └───────┬──────────┘  │
│       │              │                 │              │
└───────┼──────────────┼─────────────────┼──────────────┘
        │              │                 │
   ssh to nodes   manages processes   DNAT to backends
        │          health checks
   ┌────┴──────┐
   │  worker   │
   │  nodes    │
   └───────────┘
```

## Data Flow

1. **User** writes spec YAML → `specs/` directory on NFS.
2. **[flockd](flockd.md)** polls `specs/`, detects desired state, schedules workloads.
3. **[flockd](flockd.md)** runs `--exec-create` (e.g. `ssh {node} systemctl start podlet@{name}`).
4. **[podlet](podlet.md)** starts on the worker node, writes JSON state to stdout, heartbeat to NFS.
5. **[flockd](flockd.md)** checks health via `--exec-health`, tracks state in SQLite.
6. **[iptlb](iptlb.md)** polls `services/*.backend` files, updates iptables DNAT rules.
7. **Traffic** flows: client → VIP → iptables DNAT → backend podlet → application.

## Step-by-Step Setup

### Prerequisites

- Two or more Linux hosts with NFSv4 mount at `/mnt/state`
- SSH key-based auth from controller to all workers
- [podlet](podlet.md), [iptlb](iptlb.md), [flockd](flockd.md) binaries installed at `/usr/local/bin/`
- Systemd units installed (see [scripts/](../scripts/))

### 1. Create the Spec

Write a spec file on the NFS volume:

```bash
mkdir -p /mnt/state/specs/prod
cat > /mnt/state/specs/prod/frontend.yaml <<'EOF'
name: frontend
replicas: 3
image: nginx:alpine
cpu: 0.5
mem: "64Mi"
ports:
  - 80
health_check:
  tcp: 80
strategy:
  type: RollingUpdate
  drain_timeout: 30s
EOF
```

### 2. Define Nodes

```bash
cat > /etc/flockd/nodes <<'EOF'
worker1
worker2
worker3
EOF
```

### 3. Start Heartbeats

On every node (including controller):

```bash
systemctl enable --now podlet-heartbeat.timer
```

This writes a timestamp to `/mnt/state/heartbeats/<hostname>` every 5 seconds.

### 4. Start the Load Balancer

On the controller:

```bash
iptlb \
    --vip 10.0.0.100 \
    --port 80 \
    --backends-file /mnt/state/services/frontend.backend \
    --scheduler rr \
    --interval 2 &
```

### 5. Start the Reconciler

On the controller:

```bash
flockd \
    --specs /mnt/state/specs/prod \
    --state /mnt/state/state.db \
    --nodes-file /etc/flockd/nodes \
    --exec-create "ssh {node} systemctl start podlet@{name}" \
    --exec-delete "ssh {node} systemctl stop podlet@{name}" \
    --exec-health "test -f /mnt/state/heartbeats/{name}" \
    --scheduler first-fit \
    --lock-file /mnt/state/flockd.lock \
    --interval 5
```

Or use the [flockd-wrapper](../scripts/flockd-wrapper) for HA:

```bash
/usr/local/bin/flockd-wrapper
```

### 6. Verify

```bash
# Check flockd actions
journalctl -u flockd -f

# Check podlet instances on a worker
ssh worker1 systemctl list-units 'podlet@*'

# Check backends
cat /mnt/state/services/frontend.backend

# Test the VIP
curl http://10.0.0.100/
```

## Rolling Update

To roll out a new image, just edit the spec:

```bash
# Change the image tag
sed -i 's/nginx:alpine/nginx:1.25/' /mnt/state/specs/prod/frontend.yaml
```

[flockd](flockd.md) detects the hash change within 5 seconds and, in a single reconciliation pass:

1. Creates new instances with the updated spec.
2. Immediately drains and deletes all old instances (those with a different spec hash).

Health-based gating happens in subsequent reconciliation cycles: if not enough new instances are healthy yet, flockd creates more on the next pass. If too many instances exist, it scales down.

[iptlb](iptlb.md) picks up the updated backends file and shifts traffic without dropping connections.

## Node Failure

If a worker node goes down:

1. Its heartbeat file in `/mnt/state/heartbeats/` stops updating.
2. [flockd](flockd.md)'s `--node-health-cmd` detects the stale heartbeat.
3. The node is marked `down` in the state DB.
4. All workloads on that node are rescheduled to healthy nodes.

When the node recovers, the heartbeats resume and [flockd](flockd.md) scales down any excess replicas.

## GitOps

The `specs/` directory is a Git repository:

```bash
cd /mnt/state/specs
git init
git add .
git commit -m "initial specs"

# On the controller, enable auto-sync
systemctl enable --now git-sync.timer
```

The `git-sync` timer runs `git pull` every 30 seconds. Each branch is an environment:

```
specs/
├── prod/      # git branch: main
│   └── frontend.yaml
└── staging/   # git branch: staging
    └── frontend.yaml
```

Point [flockd](flockd.md) at the desired branch directory.

## Local E2E Test

Run the full orchestrator on a single machine (no VMs needed):

```bash
bash scripts/e2e-local.sh
```

This simulates NFS with a temp directory and uses local echo commands instead of SSH. See [scripts/e2e-local.sh](../scripts/e2e-local.sh).

## See Also

- [Quickstart](quickstart.md) — 5-minute hands-on walkthrough
- [podlet](podlet.md) — Workload supervisor
- [iptlb](iptlb.md) — L4 load balancer
- [flockd](flockd.md) — Declarative reconciler
