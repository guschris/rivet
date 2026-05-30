# flockd — Declarative Reconciler

A generic infrastructure reconciler that reads YAML spec files, compares them to a SQLite state database, and executes commands to converge reality. It is **not** Kubernetes-specific — it can reconcile anything that can be expressed as desired state + shell commands.

## Quick Start

```bash
# Create a spec
mkdir -p /mnt/state/specs
cat > /mnt/state/specs/frontend.yaml <<'EOF'
name: frontend
replicas: 2
EOF

# Create a nodes file
echo "node1" > /etc/flockd/nodes
echo "node2" >> /etc/flockd/nodes

# Start the reconciler
flockd \
    --specs /mnt/state/specs \
    --state /mnt/state/state.db \
    --nodes-file /etc/flockd/nodes \
    --exec-create "ssh {node} systemctl start podlet@{name}" \
    --exec-delete "ssh {node} systemctl stop podlet@{name}" \
    --exec-health "cat /mnt/state/heartbeats/{name}" \
    --interval 5
```

## CLI Reference

```
flockd --specs <dir>
       --state <db-path>
       [--exec-create <template>]
       [--exec-delete <template>]
       [--exec-health <template>]
       [--scheduler first-fit|best-fit]
       [--nodes-file <path>]
       [--node-health-cmd <template>]
        [--interval 5]
        [--lock-file <path>]
        [--plan-only]
        [--plan-file <path>]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--specs` | Directory of YAML spec files | *required* |
| `--state` | Path to SQLite state database | *required* |
| `--exec-create` | Command to create an instance | `echo 'created {name} on {node}'` |
| `--exec-delete` | Command to delete an instance | `echo 'deleted {name} on {node}'` |
| `--exec-health` | Command to check instance health; if empty, uses DB status (`"running"` = healthy) | *(none)* |
| `--scheduler` | `first-fit` or `best-fit` | `first-fit` |
| `--nodes-file` | File listing cluster nodes (one per line) | *(none, defaults to localhost)* |
| `--node-health-cmd` | Command to check node liveness | *(none)* |
| `--interval` | Reconciliation loop interval in seconds | `5` |
| `--lock-file` | Path for flock-based leader election | *(none)* |
| `--plan-only` | Compute plan, output JSON to stdout, exit (no execution) | *(off)* |
| `--plan-file` | Read a plan JSON file and execute its actions | *(none)* |

### Placeholder Substitution

The exec templates support `{name}` and `{node}` placeholders:

- `{name}` — the generated instance ID (e.g. `frontend-0000`)
- `{node}` — the target node hostname

Example: `ssh {node} systemctl start podlet@{name}` becomes `ssh node1 systemctl start podlet@frontend-0000`.

## Spec Format

Specs are plain YAML files (`.yaml` or `.yml`) in the `--specs` directory.

```yaml
name: frontend           # required, used as instance prefix
kind: Service            # optional, defaults to "Service"
replicas: 3              # desired instance count
cpu: 0.5                 # CPU cores per instance
mem: "64Mi"              # memory per instance
ports:                   # exposed ports
  - 80
  - 443
health_check:            # passed to podlet
  tcp: 80
strategy:                # update strategy
  type: RollingUpdate
  drain_timeout: 30s
command:                 # override container/process command
  - nginx
  - -g
  - daemon off;
```

## Reconciliation Loop

Every `--interval` seconds:

1. **Load specs** — read all `.yaml`/`.yml` files in `--specs`.
2. **Check nodes** — if `--node-health-cmd` is set, run it for each node.
3. **Reconcile per spec**:
   - Count healthy instances for this spec.
   - If fewer than `replicas`: schedule and create new instances.
   - If more than `replicas`: delete excess (unhealthy first, then oldest).
   - If spec hash changed: trigger rolling update (create new, wait healthy, drain old).

## State Database

SQLite schema (at `--state`):

```sql
CREATE TABLE instances (
    id TEXT PRIMARY KEY,       -- e.g. "frontend-0000"
    spec_name TEXT NOT NULL,   -- from spec
    node TEXT NOT NULL,        -- target node
    status TEXT NOT NULL,      -- desired, running, draining, deleting
    spec_hash TEXT NOT NULL,   -- for change detection
    created_at TEXT NOT NULL
);

CREATE TABLE nodes (
    name TEXT PRIMARY KEY,
    status TEXT NOT NULL       -- up or down
);
```

## Leader Election

For HA, wrap `flockd` with the `flockd-wrapper` script:

```bash
#!/bin/bash
flock -n /mnt/state/flockd.lock -c 'flockd --specs ... --state ...'
```

Only one `flockd` runs at a time. If the leader dies, the NFSv4 lock lease expires within seconds and a backup takes over.

## Plan Mode

Use `--plan-only` to preview what flockd **would** do without executing any commands:

```bash
flockd --specs /mnt/state/specs \
       --state /mnt/state/state.db \
       --nodes-file /etc/flockd/nodes \
       --plan-only
```

Outputs a JSON plan to stdout:

```json
[
  {"action":"Notify","message":"spec 'frontend': 0 healthy, need 2 more"},
  {"action":"Create","instance_id":"frontend-00000000","node":"node1","cmd":"ssh node1 systemctl start podlet@frontend-00000000"},
  {"action":"Create","instance_id":"frontend-00000001","node":"node2","cmd":"ssh node2 systemctl start podlet@frontend-00000001"}
]
```

All DB mutations are rolled back — `--plan-only` is safe to run on a live state database.

Use `--plan-file` to execute a previously-generated plan:

```bash
flockd --plan-only > plan.json  # generate
# review plan.json...
flockd --state /mnt/state/state.db --plan-file plan.json  # execute
```

**Note:** `--plan-file` executes commands against the current DB state, which may have changed since the plan was generated. Use it as a best-effort apply.

## See Also

- [Quickstart](quickstart.md) — 5-minute hands-on walkthrough
- [podlet](podlet.md) — The workload supervisor that flockd orchestrates
- [iptlb](iptlb.md) — Load balancer that flockd can keep updated with backend files
- [sched](sched.md) — Standalone scheduler (flockd uses the same scheduling library)
- [probe](probe.md) — Standalone health-checker for external monitoring
- [merge](merge.md) — Deep-merge tool for layering spec configs
- [Integration Guide](integration.md) — How the tools compose into a full orchestrator
