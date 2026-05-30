<p align="center">
  <b>rivet</b> — the declarative orchestrator that fits in your head
</p>

---

Kubernetes is 2.5 million lines of Go. rivet is **3,000 lines of Rust** and six composable binaries. No etcd, no API server, no RBAC, no templated YAML. Just plain files, JSON pipes, and standard Unix tools.

Each tool solves one problem and is **useful on its own** — adopt `probe` for monitoring or `merge` for config management without buying into the whole platform.

```bash
# Write a spec, merge in environment overrides
merge base.yaml --patch prod.yaml > specs/frontend.yaml

# Preview what will happen
flockd --specs specs --state state.db --nodes-file nodes --plan-only

# Run the reconciler
flockd --specs specs --state state.db --nodes-file nodes \
  --exec-create "ssh {node} podlet start {name}" \
  --exec-delete "ssh {node} podlet stop {name}"
```

```
flockd: spec 'frontend': 0 healthy, need 3 more
flockd: create: frontend-00000000 on node1 -> ssh node1 podlet start frontend-00000000
flockd: create: frontend-00000001 on node2 -> ssh node2 podlet start frontend-00000001
flockd: create: frontend-00000002 on node3 -> ssh node3 podlet start frontend-00000002
flockd: spec 'frontend': 3 replicas healthy (no change)
```

## the toolkit

| Tool | What it does | Standalone use |
|------|-------------|----------------|
| **[flockd](docs/flockd.md)** | Declarative reconciler + scheduler | Infrastructure reconciliation, DNS record maintenance |
| **[podlet](docs/podlet.md)** | Workload supervisor (process, container, VM) | Local process supervision with health checks |
| **[iptlb](docs/iptlb.md)** | File-driven L4 load balancer (iptables DNAT) | Simple TCP load balancing without nginx |
| **[merge](docs/merge.md)** | Deep-merge YAML/JSON configs (RFC 7396) | Environment-specific config overlays |
| **[probe](docs/probe.md)** | Async health-checker (TCP/HTTP/exec, NDJSON) | Uptime monitoring, circuit-breaker sidecar |
| **[sched](docs/sched.md)** | Bin-packing scheduler (first-fit, best-fit) | CI runner allocation, VM placement |

## why

- **Radical simplicity.** Debug with `cat`, `jq`, and `sqlite3`. The whole system is smaller than Kubernetes' RBAC module.
- **Zero dependencies.** A single statically-linked binary per tool. Runs on Alpine, Ubuntu, RHEL — no Python, no Bash version lock, no container runtime required.
- **Composable.** Every tool is independently useful. Use `probe` for health checks. Use `sched` for CI allocation. Use `merge` for config layering. Orchestration is a bonus.
- **No templated YAML.** Configuration is data. Logic lives in real code (`merge` for layering, `sh` for glue). No Helm charts, no `{{ .Values.foo }}`.
- **Files over APIs.** State lives in plain files on a shared volume or Git repo. No proprietary database, no gRPC, no API versioning.

## what it's not

- **Not a Kubernetes replacement.** No multi-tenancy, no cloud-provider integrations, no CRDs. Designed for clusters of <50 nodes where you control the infrastructure.
- **Not a PaaS.** It doesn't build images, provision volumes, or manage DNS. It orchestrates what's already on your nodes.

## quickstart

```bash
git clone https://github.com/anomalyco/rivet
cd rivet
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

Then walk through [docs/quickstart.md](docs/quickstart.md) — a 5-minute hands-on demo that runs entirely on a single machine.

## docs

[Quickstart](docs/quickstart.md) ·
[flockd](docs/flockd.md) ·
[podlet](docs/podlet.md) ·
[iptlb](docs/iptlb.md) ·
[merge](docs/merge.md) ·
[probe](docs/probe.md) ·
[sched](docs/sched.md) ·
[Integration Guide](docs/integration.md) ·
[Reliability Audit](docs/audit.md)

## license

MIT
