# sched — Bin-Packing Scheduler

A standalone bin-packing scheduler that maps workloads to nodes based on current load. Reads node and load data as JSON, outputs a placement decision as JSON. First‑fit and best‑fit strategies. Useful for CI/CD runner allocation, VM placement, or test sharding — without any orchestration framework.

## Quick Start

```bash
# Single placement decision from stdin
echo '{"nodes":["node1","node2","node3"],"loads":{"node1":5,"node2":2,"node3":4},"spec_name":"web","next_index":0}' | sched --strategy first-fit

# Output: {"node":"node2","instance_id":"web-00000000"}

# Read from a file
sched --strategy best-fit --input allocation.json
```

## CLI Reference

```
sched --strategy <first-fit|best-fit> [--input <FILE>]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--strategy` | Scheduling strategy: `first-fit` or `best-fit` | `first-fit` |
| `--input` | JSON input file; reads from stdin if omitted | stdin |

## Input Format

JSON on stdin or `--input` file:

```json
{
  "nodes": ["node1", "node2", "node3"],
  "loads": {
    "node1": 5,
    "node2": 2,
    "node3": 0
  },
  "spec_name": "myapp",
  "next_index": 3
}
```

| Field | Type | Description |
|-------|------|-------------|
| `nodes` | string[] | Available nodes, in priority order |
| `loads` | object | Current instance count per node (defaults to 0 for unlisted nodes) |
| `spec_name` | string | Prefix for the generated instance ID |
| `next_index` | number | Starting index for instance ID generation (default: 0) |

## Output

A single JSON object to stdout:

```json
{"node":"node2","instance_id":"myapp-00000003"}
```

Exit code 1 with an error message to stderr if no nodes are available.

## Scheduling Strategies

- **first-fit** — Picks the least-loaded node. Breaks ties by the original ordering in the `nodes` array.
- **best-fit** — Picks the least-loaded node. Breaks ties arbitrarily (sorted by load only).

Load is measured as the count of instances currently assigned to each node.

## Standalone Use Cases

- **CI runner allocation:** pipe CI job list into sched, get optimal runner assignments.
- **VM placement:** feed hypervisor capacities and current VM counts, get placement decisions.
- **Test sharding:** distribute test suites across available machines by count.

## See Also

- [Quickstart](quickstart.md) — 5-minute hands-on walkthrough
- [flockd](flockd.md) — Declarative reconciler with an integrated scheduler (uses the same library)
- [Integration Guide](integration.md) — How sched composes with the rest of the toolkit
