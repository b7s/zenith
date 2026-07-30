//! WSL (Windows Subsystem for Linux) detection for the AI-agent widget.
//!
//! When Claude Code / Codex / OpenCode are installed and running **inside a
//! WSL distro** the Windows-side transcript/db scan in `scan.rs` reports "idle",
//! because those tools write to `~/.claude` / `~/.codex` / `~/.local/share/opencode`
//! on the Linux filesystem, not under `%USERPROFILE%`.
//!
//! This module enumerates installed distros (`wsl.exe -l -q`) and walks each
//! distro's `\\wsl.localhost\<distro>\home\<user>\...` view from Windows — the
//! same filesystem Rust already uses for `scan.rs`, so we reuse those pure
//! helpers (no duplication, AGENTS §3). Per-row we tag the `title` with
//! `· WSL:<distro>` so the user sees which agent lives where in the Agents
//! window; the window already groups rows by CLI id and shows the title line,
//! so a same-CLI session on host vs WSL shows up as two rows under one panel.
//!
//! Fail-open: when `wsl.exe` is missing or no distros enumerate, every
//! function here returns an empty result — never an error — so the poll loop
//! and unit tests are unaffected on machines without WSL.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use super::model::{CliId, CliSession, CliStatus};
use super::scan;

/// How long to cache the distro list. Enumerating distros spawns `wsl.exe`
/// (≈ 50–150 ms) — pointless to repeat every 3 s poll when the set of
/// installed distros almost never changes. Falls back to a fresh enumeration
/// when the cache is older than this.
const DISTRO_TTL: Duration = Duration::from_secs(60);

/// `\\wsl.localhost` (Windows 11) supersedes the legacy `\\wsl$` share. Both
/// work on 24H2; we prefer the new name and fall back to the old one when the
/// new share is unavailable (rare on older insider builds).
const WSL_NEW_SHARE: &str = "\\\\wsl.localhost";
const WSL_OLD_SHARE: &str = "\\\\wsl$";

static DISTROS: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);

/// True when `wsl.exe` exists on PATH (or in System32). Cheap probe; safe to
/// call from the poll loop. Caches nothing — `which` check only.
pub fn enabled() -> bool {
    std::env::var("SystemRoot")
        .ok()
        .map(|r| PathBuf::from(r).join("System32").join("wsl.exe").exists())
        .unwrap_or_else(|| PathBuf::from("wsl.exe").exists() || which("wsl.exe").is_some())
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
    for dir in std::env::split_paths(&path) {
        for ext in pathext.split(';').filter(|s| !s.is_empty()) {
            let cand = dir.join(format!("{bin}{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Enumerate installed WSL distros by name, cached for `DISTRO_TTL`. Returns
/// an empty vec when WSL is absent or `wsl.exe -l -q` fails — this is a
/// fail-open probe, never an error.
///
/// `wsl.exe` writes UTF-16 LE on Windows regardless of console codepage, so we
/// decode defensively: try UTF-16 LE first, then fall back to UTF-8 lossy.
pub fn distros() -> Vec<String> {
    if let Ok(g) = DISTROS.lock() {
        if let Some((seen, list)) = &*g {
            if seen.elapsed() < DISTRO_TTL {
                return list.clone();
            }
        }
    }

    let exe = resolve_wsl_exe();
    let list = match exe {
        Some(exe) => enumerate_distros(&exe).unwrap_or_default(),
        None => Vec::new(),
    };

    if let Ok(mut g) = DISTROS.lock() {
        *g = Some((Instant::now(), list.clone()));
    }
    list
}

fn resolve_wsl_exe() -> Option<PathBuf> {
    if let Some(p) = which("wsl.exe") {
        return Some(p);
    }
    if let Ok(root) = std::env::var("SystemRoot") {
        let p = PathBuf::from(root).join("System32").join("wsl.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn enumerate_distros(exe: &Path) -> Option<Vec<String>> {
    let out = Command::new(exe)
        .args(["--list", "--quiet"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return Some(Vec::new());
    }
    let text = decode_wsl_output(&out.stdout);
    let mut names = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        // `wsl -l -q` sometimes prints a trailing "(default)" marker or Pivot
        // status; ignore any line that contains a space — names are single
        // tokens without whitespace.
        if t.chars().any(|c| c.is_whitespace()) {
            continue;
        }
        names.push(t.to_string());
    }
    Some(names)
}

/// `wsl.exe` always writes UTF-16 LE to its output handle on Windows, even
/// when the console codepage is UTF-8. Decode via the BOM/LE heuristic.
fn decode_wsl_output(bytes: &[u8]) -> String {
    // Heuristic: if every odd byte is 0 and the buffer is even-length, it's
    // almost certainly UTF-16 LE (ascii-only payload common for distro names).
    if bytes.len() >= 2
        && bytes.len().is_multiple_of(2)
        && bytes.iter().skip(1).step_by(2).all(|b| *b == 0)
    {
        let mut utf16 = Vec::with_capacity(bytes.len() / 2);
        let mut i = 0;
        while i + 1 < bytes.len() {
            utf16.push(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
            i += 2;
        }
        return String::from_utf16_lossy(&utf16);
    }
    String::from_utf8_lossy(bytes).to_string()
}

/// Resolve the Windows-side UNC prefix that lets Rust walk the distro's
/// Linux filesystem. Prefers `\\wsl.localhost`; falls back to `\\wsl$` only
/// when the new share is unreachable (older insider builds).
fn wsl_share() -> &'static str {
    if Path::new(WSL_NEW_SHARE).exists() {
        WSL_NEW_SHARE
    } else if Path::new(WSL_OLD_SHARE).exists() {
        WSL_OLD_SHARE
    } else {
        WSL_NEW_SHARE
    }
}

/// Walk `\\<share>\<distro>\home` and yield each user-home directory whose
/// `.claude`, `.codex`, or `.local/share/opencode` exists. Returns an empty
/// vec if the share or `home` is unreachable — fail-open.
fn user_homes(distro: &str) -> Vec<PathBuf> {
    let share = wsl_share();
    let home = PathBuf::from(share).join(distro).join("home");
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&home) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if !p.is_dir() {
            continue;
        }
        if has_any_agent_dir(&p) {
            out.push(p);
        }
    }
    out
}

fn has_any_agent_dir(home: &Path) -> bool {
    home.join(".claude").is_dir()
        || home.join(".codex").is_dir()
        || home.join(".local").join("share").join("opencode").is_dir()
}

/// True when the given CLI is installed in any WSL distro. Used by
/// `detect::installed_ids` to merge with the Windows-side PATH lookup so the
/// first-run seeded monitoring list also surfaces WSL-only installs.
pub fn installed_ids() -> Vec<CliId> {
    if !enabled() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for d in distros() {
        for home in user_homes(&d) {
            if home.join(".claude").join("projects").is_dir() && !out.contains(&CliId::Claude) {
                out.push(CliId::Claude);
            }
            if home.join(".codex").join("sessions").is_dir() && !out.contains(&CliId::Codex) {
                out.push(CliId::Codex);
            }
            if home
                .join(".local")
                .join("share")
                .join("opencode")
                .join("opencode.db")
                .is_file()
                && !out.contains(&CliId::Opencode)
            {
                out.push(CliId::Opencode);
            }
        }
    }
    CliId::ALL
        .iter()
        .copied()
        .filter(|id| out.contains(id))
        .collect()
}

/// Snapshot of every active AI-CLI session running inside any WSL distro.
/// Returned alongside `scan::sessions_for`'s Windows-side result so the bar
/// sees a unified picture (host + WSL rows). Each row's `title` is suffixed
/// with ` · WSL:<distro>` so the Agents window can distinguish it from a
/// co-existing host-side session of the same CLI.
pub fn scan_sessions() -> Vec<CliSession> {
    if !enabled() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for distro in distros() {
        for home in user_homes(&distro) {
            let tag = format!(" · WSL:{}", distro);
            if let Some(s) = claude_wsl_session(&home) {
                out.push(tag_session(s, &tag));
            }
            if let Some(s) = codex_wsl_session(&home) {
                out.push(tag_session(s, &tag));
            }
            out.extend(
                opencode_wsl_sessions(&home)
                    .into_iter()
                    .map(|s| tag_session(s, &tag)),
            );
        }
    }
    out
}

fn tag_session(mut s: CliSession, tag: &str) -> CliSession {
    if !s.title.is_empty() {
        s.title.push_str(tag);
    } else {
        // For idle rows we still want a visible WSL marker so the row shows up
        // distinctly in the Agents window alongside a host-side idle row.
        s.title = tag.trim_start_matches(" · ").to_string();
    }
    s
}

fn claude_wsl_session(home: &Path) -> Option<CliSession> {
    let projects = home.join(".claude").join("projects");
    if !projects.is_dir() {
        return None;
    }
    let file = scan::most_recent_match(&projects, ".*")?;
    let m = scan::mtime_ms(&file);
    let running = (scan::now_ms() - m).abs() < scan::RECENCY_MS;
    let title = scan::claude_title(&file);
    let cwd = file
        .parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let status = if running {
        scan::read_last_line(&file)
            .map(|l| scan::claude_status_from_last(&l))
            .unwrap_or(CliStatus::Idle)
    } else {
        CliStatus::Idle
    };
    Some(CliSession {
        id: CliId::Claude.as_str().into(),
        label: CliId::Claude.label().into(),
        installed: true,
        running,
        status,
        title,
        cwd,
        updated_ms: m,
        ..Default::default()
    })
}

fn codex_wsl_session(home: &Path) -> Option<CliSession> {
    let sessions = home.join(".codex").join("sessions");
    if !sessions.is_dir() {
        return None;
    }
    let file = scan::most_recent_match(&sessions, "rollout-.*")?;
    let m = scan::mtime_ms(&file);
    let running = (scan::now_ms() - m).abs() < scan::RECENCY_MS;
    let title = scan::codex_title(&file);
    let cwd = scan::codex_cwd(&file);
    let status = if running {
        scan::read_last_line(&file)
            .map(|l| scan::codex_status_from_last(&l))
            .unwrap_or(CliStatus::Idle)
    } else {
        CliStatus::Idle
    };
    Some(CliSession {
        id: CliId::Codex.as_str().into(),
        label: CliId::Codex.label().into(),
        installed: true,
        running,
        status,
        title,
        cwd,
        updated_ms: m,
        ..Default::default()
    })
}

/// OpenCode has up to N concurrent sessions per distro (mirrors the Windows
/// side). If the Linux `sqlite3` binary is absent we still surface a single
/// coarse row using the db's mtime, so the user sees the OpenCode install is
/// alive in WSL even when the SQLite rows can't be probed.
fn opencode_wsl_sessions(home: &Path) -> Vec<CliSession> {
    let db = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    if !db.is_file() {
        return Vec::new();
    }

    if let Some(rows) = scan::opencode_sessions_from_db_path(&db) {
        if !rows.is_empty() {
            return rows;
        }
    }

    // Fallback when SQLite read failed (locked by TUI, mismatched schema):
    // single coarse row using the db mtime as a liveness proxy.
    let m = scan::mtime_ms(&db);
    let running = (scan::now_ms() - m).abs() < scan::RECENCY_MS;
    vec![CliSession {
        id: CliId::Opencode.as_str().into(),
        label: CliId::Opencode.label().into(),
        installed: true,
        running,
        status: if running {
            CliStatus::Running
        } else {
            CliStatus::Idle
        },
        title: "opencode.db".into(),
        cwd: home
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        updated_ms: m,
        ..Default::default()
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_utf16_le_distro_name() {
        // "Ubuntu-24.04\n" encoded as UTF-16 LE (what `wsl.exe -l -q` writes).
        let bytes: Vec<u8> = "Ubuntu-24.04\n"
            .encode_utf16()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let s = decode_wsl_output(&bytes);
        assert_eq!(s.trim(), "Ubuntu-24.04");
    }

    #[test]
    fn decode_utf8_passthrough() {
        let s = decode_wsl_output(b"Ubuntu-24.04\n");
        assert_eq!(s.trim(), "Ubuntu-24.04");
    }

    #[test]
    fn enumerate_filters_blank_lines() {
        let lines = "Ubuntu-24.04\r\n\r\nDebian\r\n";
        let v: Vec<String> = lines
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(v, vec!["Ubuntu-24.04", "Debian"]);
    }

    #[test]
    fn tag_session_appends_to_title() {
        let s = CliSession {
            id: "claude".into(),
            label: "Claude Code".into(),
            installed: true,
            title: "Refactor the auth module".into(),
            ..Default::default()
        };
        let tagged = tag_session(s, " · WSL:Ubuntu-24.04");
        assert_eq!(tagged.title, "Refactor the auth module · WSL:Ubuntu-24.04");
    }

    #[test]
    fn tag_session_empty_title_becomes_marker() {
        let s = CliSession {
            id: "claude".into(),
            label: "Claude Code".into(),
            installed: true,
            ..Default::default()
        };
        let tagged = tag_session(s, " · WSL:Ubuntu-24.04");
        assert_eq!(tagged.title, "WSL:Ubuntu-24.04");
    }
}
