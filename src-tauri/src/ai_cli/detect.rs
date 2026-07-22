//! Pure service: probe filesystem + PATH for CLI installations.
//! Used by the widget-config window on first open to pre-fill the CLIs switch.

use std::path::PathBuf;

use super::model::{CliDetected, CliId};

/// Basic install-path conventions per CLI on Windows.
struct CliProbe {
    id: CliId,
    /// Where the config / data directory commonly lives.
    config_dir: fn() -> PathBuf,
    /// binary name as `where.exe` would find it.
    binary_name: &'static str,
    /// Optional extra directory that indicates install.
    marker_dir: Option<fn() -> PathBuf>,
}

static PROBES: &[CliProbe] = &[
    CliProbe {
        id: CliId::Opencode,
        binary_name: "opencode",
        config_dir: || {
            std::env::var("APPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir())
                .join("opencode")
        },
        marker_dir: None,
    },
    CliProbe {
        id: CliId::Claude,
        binary_name: "claude",
        config_dir: || {
            std::env::var("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir())
                .join(".claude")
        },
        marker_dir: None,
    },
    CliProbe {
        id: CliId::Codex,
        binary_name: "codex",
        config_dir: || {
            std::env::var("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir())
                .join(".codex")
        },
        marker_dir: None,
    },
];

pub fn detect_all() -> Vec<CliDetected> {
    PROBES.iter().map(probe_one).collect()
}

fn probe_one(probe: &CliProbe) -> CliDetected {
    let binary_path = resolve_binary(probe.binary_name);
    let installed = binary_path.is_some() || (probe.marker_dir.is_some_and(|d| d().exists()));
    CliDetected {
        cli_id: probe.id.as_str().into(),
        installed,
        binary_path: binary_path.unwrap_or_default(),
        config_dir: (probe.config_dir)().to_string_lossy().into(),
        version: String::new(),
    }
}

fn resolve_binary(name: &str) -> Option<String> {
    // Try `where.exe <name>` via cmd /c where
    let output = std::process::Command::new("cmd")
        .args(["/c", "where", name])
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().next().map(|l| l.trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_no_panic_on_missing() {
        let results = detect_all();
        assert_eq!(results.len(), 3);
        // Should not panic even if nothing is installed
        for r in &results {
            assert!(!r.cli_id.is_empty());
        }
    }
}
