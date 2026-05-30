# Quickstart — Orchestration in 5 Minutes

This walkthrough gets a complete declarative orchestrator running on a single machine. No Kubernetes. No etcd. No YAML templating. Just six tiny Rust binaries that fit in your head.

## 1. Build

```bash
git clone https://github.com/example/not-k8s
cd not-k8s
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

Six binaries, one command. Each under 15MB, statically linked.

```
merge   — deep-merge YAML/JSON configs
probe   — async health-checker (NDJSON streams)
sched   — bin-packing scheduler (first-fit, best-fit)
podlet  — workload supervisor (process, container, VM)
iptlb   — L4 load balancer (iptables DNAT)
flockd  — declarative reconciler
```

## 2. Write a Spec (with environment layering)

Instead of copying specs, use `merge` to layer environment overrides:

```bash
mkdir -p demo/specs

cat > demo/base.yaml <<'EOF'
name: frontend
replicas: 3
health_check:
  tcp: 80
strategy:
  type: RollingUpdate
  drain_timeout: 30s
EOF

cat > demo/prod.yaml <<'EOF'
cpu: 0.5
mem: "64Mi"
image: nginx:alpine
ports:
  - 80
  - 443
EOF

cat > demo/staging.yaml <<'EOF'
cpu: 0.25
mem: "32Mi"
replicas: 1
EOF

# Layer prod overrides onto base
merge demo/base.yaml --patch demo/prod.yaml > demo/specs/frontend.yaml
```

No `{{ .Values.foo }}`, no `helm template`, no indentation puzzles. Just data.

## 3. Preview the Plan

Before doing anything, see what `flockd` would do:

```bash
# Define our nodes
echo "localhost" > demo/nodes

flockd \
  --specs demo/specs \
  --state demo/state.db \
  --nodes-file demo/nodes \
  --exec-create "echo create {name} on {node}" \
  --exec-delete "echo delete {name} on {node}" \
  --plan-only
```

Output:

```json
[
  {"action":"Notify","message":"spec 'frontend': 0 healthy, need 3 more"},
  {"action":"Create","instance_id":"frontend-00000000","node":"localhost","cmd":"echo create frontend-00000000 on localhost"},
  {"action":"Create","instance_id":"frontend-00000001","node":"localhost","cmd":"echo create frontend-00000001 on localhost"},
  {"action":"Create","instance_id":"frontend-00000002","node":"localhost","cmd":"echo create frontend-00000002 on localhost"}
]
```

Three instances scheduled. The plan was computed exactly as production would, but the DB was rolled back — safe to run anywhere.

## 4. Run the Reconciler

Start `flockd` for real. In a terminal:

```bash
flockd \
  --specs demo/specs \
  --state demo/state.db \
  --nodes-file demo/nodes \
  --exec-create "echo create {name} on {node}" \
  --exec-delete "echo delete {name} on {node}" \
  --interval 2
```

You'll see:

```
flockd: spec 'frontend': 0 healthy, need 3 more
flockd: create: frontend-00000000 on localhost -> echo create frontend-00000000 on localhost
flockd: created: frontend-00000000
flockd: create: frontend-00000001 on localhost -> echo create frontend-00000001 on localhost
flockd: created: frontend-00000001
flockd: create: frontend-00000002 on localhost -> echo create frontend-00000002 on localhost
flockd: created: frontend-00000002
flockd: spec 'frontend': 3 replicas healthy (no change)
```

In production, `--exec-create` would be `ssh {node} systemctl start podlet@{name}`. For this demo, we're printing what would happen.

## 5. Verify Placement with `sched`

Curious where the next instance would land? Ask the scheduler directly:

```bash
echo '{"nodes":["node1","node2","node3"],"loads":{"node1":5,"node2":2},"spec_name":"api","next_index":0}' \
  | sched --strategy best-fit
```

```json
{"node":"node2","instance_id":"api-00000000"}
```

`flockd` uses the exact same scheduling logic — `sched` is the standalone version for one-off decisions or CI allocation scripts.

## 6. Health-Check with `probe`

Verify backend health independently — no orchestrator needed:

```bash
cat > demo/targets.json <<'EOF'
[
  {"name":"dns","type":"tcp","host":"8.8.8.8","port":53},
  {"name":"httpbin","type":"http","host":"httpbin.org","port":80,"path":"/status/200"}
]
EOF

probe --targets demo/targets.json --once
```

```json
{"target":"dns","type":"tcp","healthy":true,"elapsed_ms":12,"ts_ms":1717000000000}
{"target":"httpbin","type":"http","healthy":true,"elapsed_ms":234,"ts_ms":1717000000001}
```

Or stream continuously and filter with `jq`:

```bash
probe --targets demo/targets.json --interval 10 | jq 'select(.healthy == false)'
```

## 7. Rolling Update

Change the spec — `flockd` handles the rollout fully automatically. In your running `flockd` terminal:

```bash
# In another terminal, change the spec (scale down to 1 replica)
cat > demo/staging.yaml <<'EOF'
cpu: 0.25
mem: "32Mi"
replicas: 1
EOF

merge demo/base.yaml --patch demo/staging.yaml > demo/specs/frontend.yaml
```

`flockd` detects the hash change and runs the rollout over several reconcile passes:

```
flockd: spec 'frontend' changed (3 old instances), starting rollout
flockd: rollout: create 1/1 -> frontend-00000003 on localhost
flockd: rollout: 1 instances created, waiting for healthy
flockd: rollout: 1 healthy, starting drain of old instances
flockd: rollout: drain old frontend-00000000 on localhost -> echo delete...
flockd: rollout: drain old frontend-00000001 on localhost -> echo delete...
flockd: rollout: drain old frontend-00000002 on localhost -> echo delete...
flockd: spec 'frontend': 1 replicas healthy (no change)
```

One new instance created. Three old instances drained. Scale from 3→1, completely automatic. No downtime if your app handles SIGTERM gracefully.

## 8. What Just Happened?

```
                        ┌──────────────┐
   merge base + prod ──►│  specs/*.yaml │◄── git push (GitOps)
                        └──────┬───────┘
                               │ poll every 2s
                        ┌──────▼───────┐
                        │    flockd     │
                        │  reconciler   │
                        │  + scheduler  │
                        └──┬───────┬────┘
                           │       │
                    exec-create  exec-delete
                           │       │
                    ┌──────▼──┐ ┌──▼──────┐
                    │ podlet@ │ │ podlet@ │  (or ssh, kubectl, whatever)
                    │ node1   │ │ node2   │
                    └────┬────┘ └────┬───┘
                         │           │
                    ┌────▼───────────▼──┐
                    │      iptlb        │
                    │  L4 load balancer  │
                    └────────┬──────────┘
                             │
                         traffic
```

The entire control plane is under 3,000 lines of Rust. Debug with `cat`, `jq`, and `sqlite3`.

## 9. Clean Up

```bash
# Ctrl+C the running flockd
rm -rf demo
```

## Next Steps

Replace the `echo` commands with real workload supervision:

```bash
# On a real node, start the supervisor
podlet --name frontend --tcp-check :80 --restart always -- ./myapp --port 80

# Or orchestrate via SSH
flockd --exec-create "ssh {node} systemctl start podlet@{name}" \
       --exec-delete "ssh {node} systemctl stop podlet@{name}" \
       --exec-health "ssh {node} systemctl is-active podlet@{name}"
```

Read the full docs: [podlet](podlet.md) · [flockd](flockd.md) · [iptlb](iptlb.md) · [merge](merge.md) · [probe](probe.md) · [sched](sched.md) · [Integration Guide](integration.md)
