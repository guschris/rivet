use std::net::SocketAddr;
use std::path::Path;

pub fn parse_file(path: &Path) -> Result<Vec<SocketAddr>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

    let mut backends = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.parse::<SocketAddr>() {
            Ok(addr) => backends.push(addr),
            Err(_) => {
                if let Ok(addr) = format!("{}:80", line).parse::<SocketAddr>() {
                    backends.push(addr);
                } else {
                    eprintln!("iptlb: warning: invalid backend on line {}: '{}'", i + 1, line);
                }
            }
        }
    }

    Ok(backends)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_backends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backends.txt");
        std::fs::write(&path, "10.0.0.1:8080\n10.0.0.2:8080\n").unwrap();

        let backends = parse_file(&path).unwrap();
        assert_eq!(backends.len(), 2);
        assert_eq!(backends[0], "10.0.0.1:8080".parse().unwrap());
        assert_eq!(backends[1], "10.0.0.2:8080".parse().unwrap());
    }

    #[test]
    fn ignores_comments_and_blanks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backends.txt");
        std::fs::write(
            &path,
            "# this is a comment\n\n10.0.0.1:8080\n  # another comment\n10.0.0.2:9090\n",
        )
        .unwrap();

        let backends = parse_file(&path).unwrap();
        assert_eq!(backends.len(), 2);
    }

    #[test]
    fn defaults_port_80() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backends.txt");
        std::fs::write(&path, "10.0.0.1\n").unwrap();

        let backends = parse_file(&path).unwrap();
        assert_eq!(backends[0], "10.0.0.1:80".parse().unwrap());
    }

    #[test]
    fn empty_file_yields_empty_vec() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backends.txt");
        std::fs::write(&path, "").unwrap();

        let backends = parse_file(&path).unwrap();
        assert!(backends.is_empty());
    }
}
