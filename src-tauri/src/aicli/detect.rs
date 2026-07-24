//! Installed-CLI detection via PATH lookup.
//!
//! Mirrors the `resolve_bin` pattern from `git/commands.rs` (PATH + PATHEXT
//! enumeration). No process enumeration here — "running" is derived in
//! `scan.rs` from transcript-recency / OpenCode-server liveness, which is
//! more reliable for node-launched agents (claude/opencode often run under
//! `node.exe`, so image-name matching is ambiguous) and avoids `unsafe`
//! cross-process PEB walks (AGENTS §11 restricts unsafe to window/workspace/gpu).

use std::path::PathBuf;

use super::model::CliId;

/// Resolve an executable name against the process PATH, honouring the Windows
/// `PATHEXT` list so `foo`, `foo.exe`, `foo.cmd`, `foo.bat`, etc. all resolve.
/// Returns `None` when nothing matches (the CLI isn't installed).
pub fn resolve_bin(bin: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
    let exts: Vec<String> = pathext
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_uppercase())
        .collect();
    let has_ext = std::path::Path::new(bin)
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .map(|e| exts.iter().any(|x| x == &e))
        .unwrap_or(false);

    let candidates: Vec<String> = if has_ext {
        vec![bin.to_string()]
    } else {
        exts.iter().map(|e| format!("{bin}{e}")).collect()
    };

    for dir in std::env::split_paths(&path) {
        for cand in &candidates {
            let full = dir.join(cand);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

/// True when the CLI is on PATH (or shipped as a direct exe / npm shim).
pub fn is_installed(id: CliId) -> bool {
    resolve_bin(id.bin()).is_some()
}

/// Every installed CLI from the supported set.
pub fn installed_ids() -> Vec<CliId> {
    CliId::ALL.iter().copied().filter(|id| is_installed(*id)).collect()
}
