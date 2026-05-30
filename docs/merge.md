# merge — Deep Merge for YAML/JSON Configs

Deep-merges two YAML or JSON configuration files using RFC 7396 (JSON Merge Patch). Useful for layering environment-specific overrides onto a base configuration — no templating, no interpolation.

## Quick Start

```bash
# Merge a production override into a base config
merge base.yaml --patch prod-overrides.yaml

# Merge JSON files
merge base.json --patch patch.json

# Output as JSON even when inputs are YAML
merge base.yaml --patch overrides.yaml --format json
```

## CLI Reference

```
merge <BASE> --patch <PATCH> [--format yaml|json]
```

| Arg/Flag | Description | Default |
|----------|-------------|---------|
| `BASE` | Base file path, or `-` for stdin | *required* |
| `--patch` | Patch file path | *required* |
| `--format` | Output format: `yaml` or `json` | auto-detected from base extension |

## Merge Semantics (RFC 7396)

- **Objects are merged recursively.** Keys present in both base and patch are deep-merged.
- **`null` in the patch removes the key** from the base.
- **Arrays are replaced wholesale** (not merged item-by-item).
- **Scalars are replaced** by the patch value.

### Example

**base.yaml:**
```yaml
name: myapp
db:
  host: localhost
  port: 5432
secrets:
  - API_KEY
  - DB_PASS
```

**patch.yaml:**
```yaml
db:
  host: prod-db.example.com
secrets: null
replicas: 3
```

**Result:**
```yaml
name: myapp
db:
  host: prod-db.example.com
  port: 5432
replicas: 3
```

- `db.host` was overridden, `db.port` was kept.
- `secrets` was deleted (null in patch).
- `replicas` was added.

## Standalone Use Cases

- **Environment overlays:** `merge base.yaml --patch prod.yaml` for config layering.
- **Docker Compose:** `merge docker-compose.yml --patch docker-compose.override.yml`.
- **CI/CD pipelines:** merge default config with job-specific overrides.

## See Also

- [Quickstart](quickstart.md) — 5-minute hands-on walkthrough
- [flockd](flockd.md) — Declarative reconciler that consumes the resulting specs
- [Integration Guide](integration.md) — How merge fits into the GitOps workflow
