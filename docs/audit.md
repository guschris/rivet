# Reliability Audit — not-k8s

36 issues found across all three binaries and scripts.

---

## Critical (7)

### 1. flockd-wrapper double-locking deadlock
**File:** `scripts/flockd-wrapper:13-24`, `crates/flockd/src/leader.rs:5-27`

The shell wrapper runs `flock -n ${LOCK_FILE} -c 'flockd ...'`. The `flock(1)` command holds an exclusive lock on the file for the duration of the child process. Inside `flockd`, `leader::try_acquire()` opens the **same** lock file via a new fd and calls `flock(LOCK_EX | LOCK_NB)` — this gets `EWOULDBLOCK` because the shell wrapper's fd still holds the lock. Result: `flockd` always sees the lock as held, prints "another instance holds the lock, exiting", and exits with code 0. **The production deployment path is dead on arrival.**

```
flock-wrapper (pid 100)  -- holds flock() on lockfile
  └─ flockd (pid 101)   -- tries flock() on lockfile → EWOULDBLOCK → exits 0
```

**Mitigation:** Remove `--lock-file` from the wrapper's invocation of `flockd`. Either the wrapper provides leader election OR flockd's internal locking does — never both at once.

---

### 2. iptables chain flush-to-repopulate window drops all traffic
**File:** `crates/iptlb/src/rules.rs:112-115`

`apply_rules()` issues `iptables -t nat -F <chain>` first, then iterates adding rules one-by-one with `-A`. Between the flush and the last rule, the chain is empty — all connections during this window get the default chain policy (typically ACCEPT or DROP depending on kernel config). If the process or host crashes after the flush but before repopulation completes, the chain stays empty permanently.

**Mitigation:** Use `iptables-restore` to build the full ruleset in memory and apply it as a single atomic operation (`iptables-restore --noflush`).

---

### 3. NFSv4 flock() lease revocation causes split-brain
**File:** `crates/flockd/src/leader.rs:15`, `scripts/flockd-wrapper:13`

NFSv4 `flock()` locks are lease-based. If the client holding the lock experiences a network partition, the NFS server revokes the lock after the lease expires (typically 30–90s). During the gap between partition onset and lease expiry, a standby flockd on another node can acquire the lock — two reconcilers run simultaneously against the same SQLite database and exec environment. Both try to create/delete instances, leading to duplicate workloads, race conditions, and corrupted state.

**Mitigation:** Set a short NFSv4 lease time. Add a fencing token (generation number) in SQLite that the leader increments atomically on each cycle. A standby checks the generation number and refuses to operate if it matches the last known value.

---

### 4. SQLite errors silently swallowed — state inconsistency on NFS
**File:** `crates/flockd/src/reconciler.rs:72,94-97,129,175-177`

Every SQLite operation in the reconciler uses `.ok()` to discard errors:

```rust
db.insert_instance(&new_inst).ok();              // line 72
db.update_instance_status(&old.id, "draining").ok();  // line 95
exec::run_command(&cmd).ok();                    // line 96
db.delete_instance(&old.id).ok();               // line 97
```

If SQLite returns `SQLITE_BUSY` (database locked — common on NFSv4 with fcntl locking), `SQLITE_CORRUPT` (NFS write ordering violations), or `SQLITE_IOERR` (NFS stale handle), the error is swallowed. The reconciler continues as if the operation succeeded. Consequences:

- Instance created in reality but not in DB → flockd creates a duplicate next cycle
- Instance deleted from DB but not from reality → orphaned podlet never cleaned up
- Instance marked "draining" in DB but update failed → later reconciliation sees stuck "draining" record

**Mitigation:** Propagate errors from all DB operations. A failed insert should abort the current reconcile cycle. Set `busy_timeout` and use `PRAGMA journal_mode=WAL`.

---

### 5. Node health command failure defaults to healthy
**File:** `crates/flockd/src/reconciler.rs:248`

```rust
let healthy = exec::run_command(&cmd).unwrap_or(true);
```

If `exec::run_command` returns `Err` (e.g., `sh` not found, NFS mount stale, out of memory, disk full), the node is treated as **healthy**. This means:

- If the controller's NFS mount breaks, ALL nodes suddenly appear healthy
- No workloads are rescheduled even though the entire cluster may be unreachable
- flockd reports "all healthy, no change needed" during a total outage

**Mitigation:** Treat command execution failure as unhealthy: `unwrap_or(false)`.

---

### 6. Negative duration panics podlet
**File:** `crates/podlet/src/main.rs:123`

```rust
Ok(Duration::from_secs_f64(secs))
```

`parse_duration` calls `Duration::from_secs_f64(secs)`. If the user passes `--drain-timeout=-5s`, `num` parses as `-5.0`, and `from_secs_f64` **panics** (documented in stdlib). systemd's `Restart=always` with 1s delay means infinite crash loop.

**Mitigation:** Validate `num >= 0.0` before calling `from_secs_f64`, or use `Duration::try_from_secs_f64`.

---

### 7. PID reuse race in graceful_shutdown — wrong process killed
**File:** `crates/podlet/src/main.rs:400-421`

```rust
unsafe { libc::kill(pid as i32, libc::SIGTERM); }
// ... 100ms polling loop ...
while time::Instant::now() < deadline {
    let still_alive = unsafe { libc::kill(pid as i32, 0) == 0 };
    if !still_alive { return; }
}
unsafe { libc::kill(pid as i32, libc::SIGKILL); }
```

Between the child exiting and the `kill(pid, 0)` check, the kernel can recycle the PID. The subsequent `kill(pid, SIGKILL)` targets a completely unrelated process. PIDs are not stable identifiers after process exit — `waitpid`/process handles are the only reliable mechanism.

**Mitigation:** Use the tokio `Child` handle's `.wait()` with a timeout instead of PID-based polling. Or use pidfds (`pidfd_open`/`pidfd_send_signal` on Linux 5.1+).

---

## High (9)

### 8. Orphaned instances when specs are deleted — no garbage collection
**File:** `crates/flockd/src/reconciler.rs:27`

The reconciler iterates only over currently-present specs. If a YAML file is deleted via GitOps, that spec disappears from the map. Its instances in the DB and podlets on workers are never cleaned up — they become permanent zombie records consuming resources.

**Mitigation:** Add a pre-pass that queries all `spec_names` from the DB, finds those absent from `specs`, and issues `exec_delete` for their instances.

---

### 9. SQLite has no busy_timeout — NFS locking failures are fatal
**File:** `crates/flockd/src/state.rs:13`

```rust
let conn = Connection::open(path)...;
```

No `busy_timeout` is set. Combined with the `.ok()` error swallowing (#4), transient NFSv4 `fcntl()` lock recovery causes `SQLITE_BUSY` which is silently dropped. Even single-process, NFS lock recovery after a network blip causes these errors.

**Mitigation:** `conn.busy_timeout(Duration::from_secs(30))?;` plus `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;`.

---

### 10. iptables flush exit status not checked
**File:** `crates/iptlb/src/rules.rs:112-115`

```rust
Command::new("iptables").args(["-t", "nat", "-F", chain_name])
    .status()
    .map_err(|e| format!("cannot flush chain: {}", e))?;
```

`.status()` returns `Ok(ExitStatus)` even when iptables fails (non-zero exit). The `map_err` only catches spawn failures (binary not found). A failed flush (permission denied, kernel module not loaded) is silently treated as success. The function proceeds to add rules to a chain that may still have old rules → mix of old and new DNAT rules → undefined behavior.

**Mitigation:** Check `status.success()` after every iptables call.

---

### 11. allocate_random_port returns 0 on failure — podlet proceeds with invalid port
**File:** `crates/podlet/src/main.rs:382-386`

```rust
fn allocate_random_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .map(|l| l.local_addr().unwrap().port())
        .unwrap_or(0)
}
```

If binding fails (out of file descriptors, netns issues), returns port 0 — reserved and never usable. Podlet passes port 0 to the child process/container. Health checks target port 0 and never succeed.

**Mitigation:** Propagate the error upward. `allocate_ports` should return `Result`. Podlet should refuse to start with invalid port configuration.

---

### 12. println! panics on broken pipe — kills podlet and its workload
**File:** `crates/podlet/src/main.rs:157`

```rust
fn emit_state(state: &State) {
    let json = serde_json::to_string(state).unwrap();
    println!("{}", json);
}
```

If the consumer of podlet's JSON output exits (e.g., `jq`, log shipper, pipe to another process), the next `println!` panics, killing podlet and the supervised workload.

**Mitigation:** Replace `println!` with `writeln!` and handle pipe errors gracefully, or write to stderr/file.

---

### 13. Child pipe buffers not drained if podlet panics
**File:** `crates/podlet/src/main.rs:241-246`

```rust
if let Some(stdout) = child_stdout {
    tokio::spawn(forward_output(stdout, "stdout"));
}
```

The `forward_output` tasks are spawned but never joined/awaited. If podlet panics (e.g., SIGPIPE), tokio runtime shuts down, tasks cancelled mid-read. The child's pipe buffers fill up (64KB typical), child blocks on `write()` permanently — becomes an unkillable zombie.

**Mitigation:** Join the forward tasks before exiting, or set child stdout/stderr to `Stdio::null()`.

---

### 14. Double restart loop: podlet + systemd
**File:** `scripts/podlet@.service:6-9`, `crates/podlet/src/main.rs:308-318`

```
# podlet@.service
ExecStart=/usr/local/bin/podlet --name %i --restart always --max-restarts 10 ...
Restart=always
```

Both systemd (`Restart=always`) and podlet (`--restart always --max-restarts 10`) handle restarts. After 10 podlet-level restarts, podlet exits. Systemd restarts podlet with a fresh restart counter. Result: **infinite restart loop** despite max-restarts=10.

**Mitigation:** Either remove `--restart always` from podlet (let systemd handle restarts with `StartLimitBurst`), or use `Restart=no` in systemd and let podlet manage restarts.

---

### 15. cgroups v1 silently unsupported — resource limits silently absent
**File:** `crates/podlet/src/cgroups.rs:1-45`

Code writes to `/sys/fs/cgroup/<name>/cpu.max` and `memory.max` — cgroups v2 paths only. On cgroups v1 systems (older kernels, certain distro configs), these paths don't exist. Errors are caught and logged to stderr, but **podlet continues running without any resource limits**. The workload gets unrestricted CPU/memory.

**Mitigation:** On startup, check for `/sys/fs/cgroup/cgroup.controllers` (v2 signature). If absent, either exit with error or disable cgroup flags with a clear warning.

---

### 16. flockd has no SIGTERM handler — killed mid-exec with inconsistent state
**File:** `crates/flockd/src/main.rs:92-150`

flockd has no signal handling. When systemd stops it (`systemctl stop flockd`), SIGTERM arrives and flockd dies immediately. If it was mid-`exec::run_command` (e.g., `ssh {node} systemctl start podlet@{name}`), that command is orphaned — SSH keeps running, podlet starts on the worker, but flockd never records it in the DB. On restart, flockd sees no record and creates a duplicate.

**Mitigation:** Add signal handling in the main loop (similar to podlet's `select!` with `SignalKind::terminate()`). Complete current cycle before exiting.

---

## Medium (11)

### 17. git-sync hard reset silently destroys uncommitted changes
**File:** `scripts/git-sync:13`

```bash
git reset --hard "origin/$(git rev-parse --abbrev-ref HEAD)"
```

Any local modifications to spec files are silently wiped. On detached HEAD, `--abbrev-ref` returns "HEAD", producing `origin/HEAD` which may resolve to a different branch.

**Mitigation:** Stash uncommitted changes before reset. Use `git symbolic-ref --short HEAD` for safer HEAD detection. Retry on fetch failure.

---

### 18. Spec file hidden files and editor backups parsed as specs
**File:** `crates/flockd/src/spec.rs:108-114`

Files like `.frontend.yaml` (Emacs/Vim hidden), `.#spec.yaml` (Emacs lock), and `spec.backup.yaml` have `.yaml` extensions and are parsed. Hidden duplicates with the same `name` field silently overwrite each other in the HashMap (last one wins).

**Mitigation:** Skip files starting with `.`, files ending with `~`, `.swp`, `.bak`. Match only files with pattern `*.yaml` or `*.yml` and no leading dot.

---

### 19. HashMap iteration order makes reconcile non-deterministic
**File:** `crates/flockd/src/reconciler.rs:27`

```rust
for spec in specs.values() {
```

`HashMap` iteration order is random (seeded per process). If two specs have cross-dependencies (database before app), the order of creation is unpredictable — sometimes app starts before database, sometimes after.

**Mitigation:** Use `BTreeMap` for deterministic ordering, or sort keys before iterating.

---

### 20. spec_hash uses unwrap_or_default — silent hash collision on serialization failure
**File:** `crates/flockd/src/spec.rs:84`

```rust
let json = serde_json::to_string(spec).unwrap_or_default();
```

If serialization fails, the hash is computed on an empty string. Two different specs that both fail to serialize produce identical hashes (`e3b0c442...`), preventing change detection for both.

**Mitigation:** Return `Result` and propagate the serialization error.

---

### 21. Cgroup directory never cleaned up
**File:** `crates/podlet/src/cgroups.rs:9-10`

The cgroup directory at `/sys/fs/cgroup/<name>/` is created but never deleted on shutdown. Stale directories accumulate over repeated podlet invocations with the same name.

**Mitigation:** Remove the cgroup directory on podlet exit after the child is reaped.

---

### 22. Heartbeat file written non-atomically on NFS
**File:** `scripts/podlet-heartbeat:7-8`

```bash
date +%s > "${STATE_DIR}/heartbeats/${HOSTNAME}"
```

`>` truncates the file then writes. On NFSv4, this is not atomic. flockd reading during the window gets a truncated (0-byte) file.

**Mitigation:** Write to a temp file and `mv` (atomic on NFSv4):
```bash
date +%s > "${STATE_DIR}/heartbeats/${HOSTNAME}.tmp"
mv "${STATE_DIR}/heartbeats/${HOSTNAME}.tmp" "${STATE_DIR}/heartbeats/${HOSTNAME}"
```

---

### 23. Workload name not validated — used in filesystem paths and iptables
**File:** `crates/podlet/src/cgroups.rs:4-6`, `crates/podlet/src/workload.rs:53`, `crates/iptlb/src/rules.rs:4-6`

`--name` is used directly as a directory name (`/sys/fs/cgroup/<name>/`), container name, and in iptables chain names. If name contains `/` or `..`, path traversal is possible. iptables chain names have a 29-char limit.

**Mitigation:** Validate name against `^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,62}[a-zA-Z0-9]$` at parse time. Reject names with `/`, `..`, or shell metacharacters.

---

### 24. iptlb chain name collision for similar VIPs
**File:** `crates/iptlb/src/rules.rs:4-6`

```rust
format!("IPTLB-{}", vip.replace(['.', ':'], "-"))
```

`10.0.0.1` → `IPTLB-10-0-0-1` and `10:0:0:1` (IPv6 compressed) → `IPTLB-10-0-0-1`. Two iptlb instances with different VIPs could collide on chain names.

**Mitigation:** Include a port or hash in the chain name: `IPTLB-{port}-{hash}`.

---

### 25. Instance ID overflows after 65535 instances per spec
**File:** `crates/flockd/src/scheduler.rs:43`

```rust
let instance_id = format!("{}-{:04x}", spec_name, next_index);
```

`{:04x}` supports exactly 65535 unique IDs. In high-churn clusters with frequent rolling updates, this could exhaust. The hex becomes 5+ chars, breaking operator expectations of fixed-width IDs.

**Mitigation:** Use `{:08x}` or skip fixed-width padding.

---

### 26. git-sync runs while flockd reads specs — TOCTOU race
**File:** `scripts/git-sync:12-13`, `crates/flockd/src/spec.rs:98-128`

`git-sync` runs `git reset --hard` file-by-file while `flockd` reads spec files concurrently. Rare but possible race: flockd starts reading `frontend.yaml`, git-sync deletes it during `reset --hard`, flockd gets `ENOENT` or partial read.

**Mitigation:** `git-sync` should write to a staging directory and atomically rename, or flockd should lock the specs directory during read.

---

### 27. "desired" status counted as healthy without exec-health
**File:** `crates/flockd/src/reconciler.rs:226-229`

```rust
if exec_health_template.is_empty() {
    return inst.status == "running" || inst.status == "desired";
}
```

When no `--exec-health` is configured, pending instances (status "desired", not yet confirmed running) count as healthy. This means flockd may believe 3 replicas are healthy when only 1 is actually running.

**Mitigation:** Only count "running" status as healthy. Track "confirmed" vs "pending" separately.

---

## Low (9)

### 28. Exit code -1 after signal-killed child
**File:** `crates/podlet/src/main.rs:254`

```rust
let code = status.unwrap().code().unwrap_or(-1);
```

If the child is killed by a signal, `code()` returns `None`. `unwrap_or(-1)` yields exit code -1 which wraps to 255 (u8). Systemd misinterprets this as a generic failure. **Mitigation:** Distinguish signal exit vs clean exit.

---

### 29. ensure_chain conflates all iptables errors
**File:** `crates/iptlb/src/rules.rs:59-78`

If `iptables -t nat -L <chain>` fails for any reason (permissions, kernel module), the code assumes "chain doesn't exist" and tries `-N`. Both commands fail, with a misleading error message. **Mitigation:** Distinguish missing-chain errors from permission/availability errors.

---

### 30. lock_guard.take() on DB failure is unnecessary
**File:** `crates/flockd/src/main.rs:81-84`

`leader::release(f)` just calls `drop(_file)`, which would happen when `lock_guard` goes out of scope anyway. Dead/useless code, not harmful.

---

### 31. max_load_per_node CLI flag never used
**File:** `crates/flockd/src/main.rs:49`

```rust
#[arg(long)]
max_load_per_node: Option<u32>,
```

Parsed but never referenced anywhere in the code. Confuses users who expect it to work. **Mitigation:** Implement or remove.

---

### 32. Instance IDs recycle after scale-down
**File:** `crates/flockd/src/reconciler.rs:113,59`

`next_idx` starts at `instances.len()`. When instances are deleted (scale-down), `len()` decreases. New instances reuse old IDs — not monotonic. Mostly harmless but breaks ID-as-total-creations semantics.

---

### 33. e2e tests use PID-based kill (same race as #7)
**File:** `scripts/e2e-local.sh:210-212`

Low severity in test environment where PID reuse is improbable.

---

### 34. Spec `replicas: 0` with zero existing instances logs "no change" every cycle
**File:** `crates/flockd/src/reconciler.rs:147-184`

No functional bug, but unnecessary log noise every 5 seconds.

---

### 35. podlet exit_code unused assignment warning on return path
**File:** `crates/podlet/src/main.rs:254-321`

Compiler correctly identifies that `exit_code` is assigned but the value is never read when the `return` in the ctrl_c handler is taken. Not a bug, but a minor code smell.

---

### 36. flockd no health check for exec commands that return non-zero
**File:** `crates/flockd/src/reconciler.rs:247-249`

`check_health` returns false on non-zero exec exit, but the caller (`reconcile`) uses this correctly — unhealthy instances are counted separately from running. No actual bug, but the return value semantics could be inverted.

---

## Summary

| Severity | Count | Key Themes |
|----------|-------|------------|
| **Critical** | 7 | Double-locking, iptables flush-race, NFS split-brain, SQLite-on-NFS corruption, negative duration panic, PID reuse |
| **High** | 9 | Orphaned instances, SQLite busy timeout, flush exit unchecked, port 0 fallback, SIGPIPE panic, pipe leaks, double restart, cgroups v1, no signal handling |
| **Medium** | 11 | git-sync destructive, hidden files, non-deterministic ordering, serialization hash, cgroup cleanup, non-atomic heartbeat, name validation, chain collision, ID overflow, TOCTOU, desired-as-healthy |
| **Low** | 9 | Exit code -1, error conflation, dead code, PID race in tests, replicas:0 log noise |
