# Implementation Plan: microk8s-like Unix Toolkit (Rust)

## Testing Strategy

| Layer | What | How |
|-------|------|-----|
| **Integration tests** (`tests/` in each crate) | Real binaries, real processes, real files, real sockets | `assert_cmd` for CLI testing, `tempfile` for isolated state, actual `Command::spawn` |
| **Privileged tests** | cgroups, iptables, podman — gate behind `#[ignore]` or env var | Run in CI with proper capabilities |
| **E2E system tests** (Phase 6) | Multi-node NFSv4 scenario with rolling updates and node failure | Vagrant/cloud VMs |

Key principle: tests exercise the actual binary, not mocked internals.

---

## Phase 0 — Workspace Setup + Test Infrastructure
- Root `Cargo.toml` workspace
- `crates/podlet/`, `crates/iptlb/`, `crates/flockd/`
- Dev-dependencies added upfront: `assert_cmd`, `predicates`, `tempfile`, `tokio-test`
- Each crate gets a `tests/` directory scaffolded alongside `src/`

## Phase 1 — `podlet` + Tests (~800 lines + ~300 lines tests)

**Code modules**: `main.rs`, `workload.rs`, `cgroups.rs`, `health.rs`, `ports.rs`, `signal.rs`

**Integration tests** (`crates/podlet/tests/`):

| Test file | What it validates | Privileged? |
|-----------|------------------|-------------|
| `smoke.rs` | `podlet --help` prints usage; `podlet --name test -- sleep 1` exits cleanly; JSON line emitted to stdout | No |
| `lifecycle.rs` | Starts a process, reads JSON from stdout, sees `{"status":"running"}` → process dies → sees `{"status":"exited"}` | No |
| `health_tcp.rs` | Starts a tiny embedded TCP listener, runs podlet with `--tcp-check :PORT`, verifies `"health":"healthy"` appears in JSON output | No |
| `health_http.rs` | Starts tiny HTTP server, runs podlet with `--http-check /`, verifies healthy | No |
| `health_exec.rs` | Runs podlet with `--exec-check "true"`, verifies healthy; with `--exec-check "false"`, verifies unhealthy | No |
| `ports.rs` | Runs podlet with `--ports 8080`, verifies JSON contains a port mapping with a host port assigned | No |
| `signal_drain.rs` | Sends SIGTERM to podlet, verifies child gets SIGTERM first, then SIGKILL after drain timeout | No |
| `restart_policy.rs` | `--restart always --max-restarts 3`: kills child multiple times, verifies podlet restarts it then stops after 3 | No |
| `cgroups.rs` | `--cpu 0.5 --mem 64M`: verifies `/sys/fs/cgroup/<name>/cpu.max` and `memory.max` contain correct values | Yes (root) |
| `podman_smoke.rs` | `--type container -- docker.io/alpine:latest echo hi`: verifies podman runs and exits cleanly | Yes (podman) |

## Phase 2 — `iptlb` + Tests (~300 lines + ~200 lines tests)

**Code modules**: `main.rs`, `backend.rs`, `rules.rs`

**Integration tests** (`crates/iptlb/tests/`):

| Test file | What it validates | Privileged? |
|-----------|------------------|-------------|
| `backend_parsing.rs` | Various backends file formats parsed correctly (comments, blank lines, invalid lines rejected) | No |
| `rules_generation.rs` | Given a backends file, verify the generated `iptables-restore` input is correct for round-robin and least-connection modes | No |
| `change_detection.rs` | Write backends file, run iptlb, verify it detects the file; modify file, verify it regenerates rules | No |
| `iptables_apply.rs` | Actually applies DNAT rules in a network namespace, verifies with `iptables -t nat -L` | Yes (root + netns) |
| `connection_preservation.rs` | Validates existing connections aren't dropped during rule swap (checks `iptables-restore -n`) | Yes (root + netns) |

## Phase 3 — `flockd` + Tests (~1200 lines + ~400 lines tests)

**Code modules**: `main.rs`, `specs.rs`, `state.rs`, `reconciler.rs`, `scheduler.rs`, `rolling.rs`, `exec.rs`, `leader.rs`

**Integration tests** (`crates/flockd/tests/`):

| Test file | What it validates | Privileged? |
|-----------|------------------|-------------|
| `spec_parsing.rs` | YAML specs with various fields parse correctly; missing required fields produce errors | No |
| `basic_reconcile.rs` | Creates a spec with `replicas: 2`, runs flockd against empty state, verifies exec-create is called twice | No |
| `scale_down.rs` | Has 3 replicas in state, spec says 1, verifies exec-delete called on 2 (preferring unhealthy) | No |
| `scheduler_first_fit.rs` | Nodes file with varying capacities, multiple specs, verifies workloads land on expected nodes | No |
| `scheduler_best_fit.rs` | Same inputs but `--scheduler best-fit`, verifies different packing | No |
| `rolling_update.rs` | Spec with image v1 running, change spec to v2, verify: new replica created → health polled → old drained → old deleted | No |
| `node_failure.rs` | `--node-health-cmd` returns failure for one node, verifies workloads on that node are rescheduled | No |
| `leader_election.rs` | Two flockd processes start, one acquires lock, other exits. Kill the leader, backup acquires lock and resumes | No |
| `spec_change_detection.rs` | Modify spec file in-place, verify hash change is detected | No |
| `state_db_persistence.rs` | Run flockd, create state, restart flockd, verify state is reloaded correctly from SQLite | No |

## Phase 4 — Glue Scripts + Systemd Units (~100 lines)
- `podlet-heartbeat`, `flockd-wrapper`, `git-sync.timer`/`.service`, `podlet@.service`
- Tests: shell-based integration tests that verify systemd unit starts/stops podlet correctly

## Phase 5 — Optional Stretch (specmerge, submit-job)

## Dependency Map

```
Phase 0 ──► Phase 1 (podlet) ──► Phase 2 (iptlb) ──► Phase 3 (flockd) ──► Phase 4 (glue)
                                     │
                                     └── (podlet and iptlb are independent of each other)
```
