use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub replicas: u32,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub cpu: Option<f64>,
    #[serde(default)]
    pub mem: Option<String>,
    #[serde(default)]
    pub ports: Vec<u16>,
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
    #[serde(default)]
    pub strategy: Option<Strategy>,
}

fn default_kind() -> String {
    "Service".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    #[serde(default)]
    pub tcp: Option<u16>,
    #[serde(default)]
    pub http: Option<String>,
    #[serde(default)]
    pub exec: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    #[serde(rename = "type", default = "default_strategy_type")]
    pub type_: String,
    #[serde(default)]
    pub drain_timeout: Option<String>,
}

fn default_strategy_type() -> String {
    "Recreate".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Instance {
    pub id: String,
    pub spec_name: String,
    pub node: String,
    pub status: String,
    pub spec_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    pub name: String,
    pub status: String,
}

pub fn parse_spec_file(path: &Path) -> Result<Spec, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    parse_spec(&content)
}

pub fn parse_spec(content: &str) -> Result<Spec, String> {
    serde_yaml::from_str::<Spec>(content)
        .map_err(|e| format!("invalid spec YAML: {}", e))
}

pub fn hash_spec(spec: &Spec) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let json = serde_json::to_string(spec)
        .map_err(|e| format!("serialization error: {}", e))?;
    hasher.update(json.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

#[allow(dead_code)]
pub fn hash_file(path: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn load_specs(specs_dir: &Path) -> Result<BTreeMap<String, Spec>, String> {
    let mut specs = BTreeMap::new();

    let entries = std::fs::read_dir(specs_dir)
        .map_err(|e| format!("cannot read specs dir {}: {}", specs_dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry error: {}", e))?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if filename.starts_with('.') {
            continue;
        }
        if filename.ends_with('~') || filename.ends_with(".swp") || filename.ends_with(".bak") {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }

        match parse_spec_file(&path) {
            Ok(spec) => {
                specs.insert(spec.name.clone(), spec);
            }
            Err(e) => {
                eprintln!("flockd: warning: skipping {}: {}", path.display(), e);
            }
        }
    }

    Ok(specs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_spec() {
        let yaml = "name: test\nreplicas: 2\n";
        let spec = parse_spec(yaml).unwrap();
        assert_eq!(spec.name, "test");
        assert_eq!(spec.replicas, 2);
        assert_eq!(spec.kind, "Service");
    }

    #[test]
    fn parses_full_spec() {
        let yaml = r#"
name: frontend
kind: Service
replicas: 3
image: nginx:latest
cpu: 0.5
mem: "64Mi"
ports:
  - 80
  - 443
health_check:
  tcp: 80
strategy:
  type: RollingUpdate
  drain_timeout: 30s
command:
  - nginx
  - -g
  - daemon off;
"#;
        let spec = parse_spec(yaml).unwrap();
        assert_eq!(spec.replicas, 3);
        assert_eq!(spec.ports.len(), 2);
        assert!(spec.health_check.is_some());
        assert_eq!(spec.health_check.unwrap().tcp, Some(80));
        assert_eq!(spec.strategy.unwrap().type_, "RollingUpdate");
    }

    #[test]
    fn hash_changes_on_content_change() {
        let spec1 = parse_spec("name: test\nreplicas: 1\n").unwrap();
        let spec2 = parse_spec("name: test\nreplicas: 2\n").unwrap();
        assert_ne!(hash_spec(&spec1).unwrap(), hash_spec(&spec2).unwrap());
    }

    #[test]
    fn hash_stable_for_same_spec() {
        let spec1 = parse_spec("name: test\nreplicas: 1\n").unwrap();
        let spec2 = parse_spec("name: test\nreplicas: 1\n").unwrap();
        assert_eq!(hash_spec(&spec1).unwrap(), hash_spec(&spec2).unwrap());
    }
}
