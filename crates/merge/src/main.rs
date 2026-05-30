use clap::Parser;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "merge",
    about = "Deep-merge YAML/JSON configs using RFC 7396 (JSON Merge Patch)"
)]
struct Cli {
    #[arg(value_name = "BASE", help = "Base file path, or '-' for stdin")]
    base: String,

    #[arg(long, value_name = "PATCH", help = "Patch file path")]
    patch: PathBuf,

    #[arg(
        long,
        value_name = "FORMAT",
        help = "Output format: yaml or json (auto-detected from base extension if omitted)"
    )]
    format: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let base_value = read_input(&cli.base).unwrap_or_else(|e| {
        eprintln!("merge: error reading base: {}", e);
        std::process::exit(1);
    });

    let patch_value = read_file(&cli.patch).unwrap_or_else(|e| {
        eprintln!("merge: error reading patch: {}", e);
        std::process::exit(1);
    });

    let mut result = base_value;
    merge(&mut result, &patch_value);

    let out_format = cli
        .format
        .unwrap_or_else(|| detect_format(&cli.base));

    write_output(&result, &out_format).unwrap_or_else(|e| {
        eprintln!("merge: error writing output: {}", e);
        std::process::exit(1);
    });
}

fn detect_format(path: &str) -> String {
    if path.ends_with(".json") {
        "json".into()
    } else {
        "yaml".into()
    }
}

fn read_input(path: &str) -> Result<serde_json::Value, String> {
    if path == "-" {
        let reader = BufReader::new(io::stdin());
        serde_yaml::from_reader(reader).map_err(|e| format!("stdin: {}", e))
    } else {
        read_file(&PathBuf::from(path))
    }
}

fn read_file(path: &PathBuf) -> Result<serde_json::Value, String> {
    let file = File::open(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let path_str = path.to_string_lossy();
    if path_str.ends_with(".json") {
        serde_json::from_reader(reader).map_err(|e| format!("{}: {}", path.display(), e))
    } else {
        serde_yaml::from_reader(reader).map_err(|e| format!("{}: {}", path.display(), e))
    }
}

fn write_output(value: &serde_json::Value, format: &str) -> io::Result<()> {
    let stdout = io::stdout();
    if format == "json" {
        serde_json::to_writer_pretty(stdout, value)?;
    } else {
        serde_yaml::to_writer(stdout, value)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    }
    Ok(())
}

fn merge(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                if value.is_null() {
                    base_map.remove(key);
                } else if let Some(base_val) = base_map.get_mut(key) {
                    merge(base_val, value);
                } else {
                    base_map.insert(key.clone(), value.clone());
                }
            }
        }
        (base, patch) => {
            *base = patch.clone();
        }
    }
}
