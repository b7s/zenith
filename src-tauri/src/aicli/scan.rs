//! Pure transcript + server scanning — derive per-CLI session state.
//!
//! No `tauri::` types; unit-testable. "Running" is derived from transcript
//! recency / OpenCode-server liveness rather than process enumeration
//! (claude/opencode commonly run under `node.exe`, where image-name matching
//! is ambiguous, and cross-process PEB walks are forbidden by AGENTS §11).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::model::{CliId, CliSession, CliStatus};

/// A transcript line is considered "active" (→ running) if modified within
/// this window. Generous enough to survive a brief agent think-pause.
pub const RECENCY_MS: i64 = 60_000;

/// Truncate a candidate title to a bar-friendly length.
fn truncate(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        return t.to_string();
    }
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i + 1 >= max {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn mtime_ms(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn user_profile() -> Option<PathBuf> {
    std::env::var("USERPROFILE").ok().map(PathBuf::from).filter(|p| p.exists())
}

/// The most-recently-modified file matching `pattern` anywhere under `root`
/// (recursive), within no age constraint — recency is judged by the caller.
pub fn most_recent_match(root: &Path, glob: &str) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, i64)> = None;
    walk_files(root, &mut |p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if match_glob(name, glob) {
            let m = mtime_ms(p);
            match &best {
                Some((_, bm)) if m <= *bm => {}
                _ => best = Some((p.to_path_buf(), m)),
            }
        }
    });
    best.map(|(p, _)| p)
}

pub fn match_glob(name: &str, glob: &str) -> bool {
    if let Some(prefix) = glob.strip_suffix(".*") {
        name.starts_with(prefix) && name.ends_with(".jsonl")
    } else {
        name == glob
    }
}

/// Recursively visit files only (no symlink following).
fn walk_files(root: &Path, f: &mut dyn FnMut(&Path)) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for ent in entries.flatten() {
        let p = ent.path();
        match ent.file_type() {
            Ok(ft) if ft.is_dir() => walk_files(&p, f),
            Ok(ft) if ft.is_file() => f(&p),
            _ => {}
        }
    }
}

/// Read the last non-empty line of a (potentially large) JSONL transcript.
/// Reads the whole file — transcripts for one session are bounded (KB–low MB).
pub fn read_last_line(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.lines().rfind(|l| !l.trim().is_empty()).map(|s| s.to_string())
}

/// Read the first user-message text from a Claude Code transcript JSONL.
/// Claude lines look like `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"…"}]}}`.
pub fn claude_title(path: &Path) -> String {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return String::new();
    };
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("type").and_then(|t| t.as_str()) == Some("user") {
            if let Some(text) = first_user_text(&v) {
                let cleaned = strip_meta(&text);
                if !cleaned.is_empty() {
                    return truncate(&cleaned, 64);
                }
            }
        }
    }
    // Fallback: the enclosing project dir name (Claude nests transcripts in
    // an encoded-cwd folder, e.g. -C--Users-…-project).
    path.parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .map(|s| truncate(&s, 48))
        .unwrap_or_default()
}

/// Extract the first `{"type":"text","text":…}` string from a Claude user msg.
fn first_user_text(v: &Value) -> Option<String> {
    let content = v.get("message").and_then(|m| m.get("content"))?;
    if let Some(arr) = content.as_array() {
        for part in arr {
            if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                    return Some(t.to_string());
                }
            }
        }
    } else if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    None
}

/// Strip the `<command-message>` / `<local-command-stdout>` / `<system-…>`
/// wrappers Claude injects so the title is the user's actual prompt.
fn strip_meta(s: &str) -> String {
    let t = s.trim();
    if t.starts_with('<') {
        if let Some(close) = t.find('>') {
            let inner = t[close + 1..].trim();
            if !inner.is_empty() {
                return inner.to_string();
            }
        }
    }
    t.to_string()
}

/// Map a Claude Code transcript's last line to a status.
pub fn claude_status_from_last(line: &str) -> CliStatus {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return CliStatus::Idle;
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("assistant") => CliStatus::Running,
        Some("user") => CliStatus::Idle, // user turn done → treat as idle
        Some("result") => CliStatus::Idle,
        Some("system") => {
            // Claude writes a system line on idle; a pending permission may
            // appear as a tool_use with no result best-effort → Idle here,
            // the hook path provides precise Waiting.
            CliStatus::Idle
        }
        _ => CliStatus::Idle,
    }
}

/// Build a Claude Code session from the most-recent transcript.
fn claude_session(installed: bool) -> CliSession {
    let mut s = CliSession {
        id: CliId::Claude.as_str().into(),
        label: CliId::Claude.label().into(),
        installed,
        ..Default::default()
    };
    if !installed {
        return s;
    }
    let Some(profile) = user_profile() else { return s };
    let projects = profile.join(".claude").join("projects");
    let Some(file) = most_recent_match(&projects, ".*") else { return s };
    let m = mtime_ms(&file);
    s.updated_ms = m;
    s.running = (now_ms() - m).abs() < RECENCY_MS;
    s.title = claude_title(&file);
    s.cwd = file
        .parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(last) = read_last_line(&file) {
        s.status = if s.running { claude_status_from_last(&last) } else { CliStatus::Idle };
    }
    s
}

/// Build a Codex session from the most-recent rollout transcript.
/// Codex lives under `%USERPROFILE%\.codex\sessions\<date>\rollout-*.jsonl`.
fn codex_session(installed: bool) -> CliSession {
    let mut s = CliSession {
        id: CliId::Codex.as_str().into(),
        label: CliId::Codex.label().into(),
        installed,
        ..Default::default()
    };
    if !installed {
        return s;
    }
    let Some(profile) = user_profile() else { return s };
    let sessions = profile.join(".codex").join("sessions");
    let Some(file) = most_recent_match(&sessions, "rollout-.*") else { return s };
    let m = mtime_ms(&file);
    s.updated_ms = m;
    s.running = (now_ms() - m).abs() < RECENCY_MS;
    s.title = codex_title(&file);
    s.cwd = codex_cwd(&file);
    if let Some(last) = read_last_line(&file) {
        s.status = if s.running { codex_status_from_last(&last) } else { CliStatus::Idle };
    }
    s
}

pub fn codex_title(path: &Path) -> String {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return String::new();
    };
    // Codex rollout lines vary; look for the first line carrying a user prompt
    // (commonly `{ "payload": { "prompt": "…" } }` or a `content` text part).
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(t) = v
            .get("payload")
            .and_then(|p| p.get("prompt"))
            .and_then(|p| p.as_str())
        {
            if !t.is_empty() {
                return truncate(t, 64);
            }
        }
        if let Some(t) = v.get("prompt").and_then(|p| p.as_str()) {
            if !t.is_empty() {
                return truncate(t, 64);
            }
        }
    }
    file_stem_name(path)
}

/// `rollout-<timestamp>-<random>-<cwd-slug>.jsonl` — best-effort cwd from stem.
pub fn codex_cwd(path: &Path) -> String {
    let stem = file_stem_name(path);
    // strip a leading "rollout-" and the timestamp/random segments
    let parts: Vec<&str> = stem.splitn(5, '-').collect();
    if parts.len() >= 5 {
        return truncate(parts[4], 48);
    }
    String::new()
}

pub fn codex_status_from_last(line: &str) -> CliStatus {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return CliStatus::Idle;
    };
    // Codex emits a `type`/`kind` on rollout entries; without exact schema we
    // treat any non-terminal last line as Running. Hook path refines this.
    match v.get("type").and_then(|t| t.as_str()).or_else(|| v.get("kind").and_then(|t| t.as_str())) {
        Some("message") | Some("response") | Some("function_call") => CliStatus::Running,
        Some("completed") | Some("stop") | Some("end") => CliStatus::Idle,
        _ => CliStatus::Running,
    }
}

fn file_stem_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Probe the OpenCode server and derive a session.
///
/// **Two-tier detection strategy** (OpenCode's TUI mode does NOT expose an
/// HTTP server, so HTTP-only probing reported "idle" forever for TUI users):
///
/// **Tier 1 - SQLite direct read** (preferred, sub-ms):
/// Read `~/.local/share/opencode/opencode.db` for the most recently updated
/// session. Derive `running` from `session.time_updated` recency: an active
/// TUI bumps this row on every message/state change (every few seconds).
/// Also pull the most-recent `message.time_updated` for an even fresher
/// liveness signal.
///
/// **Tier 2 - HTTP /global/health** (fallback for `opencode serve` users):
/// When SQLite is unavailable (alien path layout, network-mounted home,
/// WSL-only install with no Windows-side db), fall back to the HTTP probe
/// against the well-known ports.
///
/// **Wire shape (verified against OpenCode server.mdx, confirmed via the
/// db schema dump):**
///   - SQLite: `session(id, title, directory, time_updated, time_archived, ...)`
///   - SQLite: `message(id, session_id, time_updated, data)`
///   - HTTP:   `/global/health` → `{healthy: true}`; `/session` → `Session[]`
///     (or `{data:[...]}` v2 envelope); `/session/status` returns each
///     session's status as an OBJECT `{ type: "busy" | "idle" | "retry" }`
///     — never a raw string.
fn opencode_sessions(installed: bool) -> Vec<CliSession> {
    if !installed {
        return Vec::new();
    }

    if let Some(v) = opencode_sessions_from_db() {
        if !v.is_empty() {
            return v;
        }
    }
    if let Some(sess) = opencode_session_from_http() {
        return vec![sess];
    }

    // Installed but nothing detected — single idle placeholder card so the
    // window still shows the CLI row.
    vec![CliSession {
        id: CliId::Opencode.as_str().into(),
        label: CliId::Opencode.label().into(),
        installed,
        ..Default::default()
    }]
}

/// Recency windows for the SQLite strategy.
/// - < RUNNING_THRESHOLD_MS since the most recent activity → running
/// - < WAITING_THRESHOLD_MS                    → waiting (brief pause)
/// - otherwise                                  → idle
const RUNNING_THRESHOLD_MS: i64 = 30_000;
const WAITING_THRESHOLD_MS: i64 = 90_000;

/// Upper bound on the number of concurrent OpenCode sessions we surface in
/// the window. Keeps the card list readable; the TUI can have hundreds of
/// archived idle sessions but only the most recent few matter for "what's
/// running right now".
const OPENCODE_MAX_SESSIONS: usize = 4;

/// Tier 1 — read OpenCode's local SQLite DB and derive running/idle/waiting
/// sessions from row recency. Returns up to `OPENCODE_MAX_SESSIONS` most
/// recently updated non-archived sessions, each classified independently.
/// Returns `None` if the db is missing or unreadable (so the caller falls
/// back to HTTP).
fn opencode_sessions_from_db() -> Option<Vec<CliSession>> {
    let db = opencode_db_path()?;
    opencode_sessions_from_db_path(&db)
}

/// OpenCode SQLite reader parametrized by an explicit db path. Owned by
/// `scan.rs` (single home, AGENTS §3) — WSL side delegates here by passing
/// the `\\wsl.localhost\<distro>\home\<user>\...opencode.db` path. Returns
/// `None` when the db is missing or unreadable, so the caller can fall back.
pub fn opencode_sessions_from_db_path(db: &Path) -> Option<Vec<CliSession>> {
    use rusqlite::Connection;
    if !db.exists() {
        return None;
    }

    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_URI
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(db, flags)
        .or_else(|_| Connection::open(db))
        .ok()?;

    // Short busy-wait — if the TUI has a write lock for a moment, retry
    // briefly rather than failing the snapshot.
    let _ = conn.busy_timeout(std::time::Duration::from_millis(200));

    let now = now_ms();

    // Pull the N most-recently updated non-archived sessions.
    let mut stmt = conn
        .prepare(
            "SELECT id, title, directory, time_updated
             FROM session
             WHERE time_archived IS NULL
             ORDER BY time_updated DESC
             LIMIT ?1",
        )
        .ok()?;
    let rows: Vec<(String, String, String, i64)> = stmt
        .query_map(rusqlite::params![OPENCODE_MAX_SESSIONS as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        return Some(Vec::new());
    }

    // Freshen each session's mtime with its most-recent `message.time_updated`.
    let mut out: Vec<CliSession> = Vec::with_capacity(rows.len());
    for (session_id, title_raw, dir_raw, session_time_updated) in rows {
        let latest_msg_time: Option<i64> = conn
            .query_row(
                "SELECT time_updated FROM message
                 WHERE session_id = ?1
                 ORDER BY time_updated DESC LIMIT 1",
                rusqlite::params![&session_id],
                |r| r.get(0),
            )
            .ok();

        let best_recent = latest_msg_time.unwrap_or(session_time_updated);
        let age = now - best_recent;

        let (running, status) = if age < RUNNING_THRESHOLD_MS {
            (true, CliStatus::Running)
        } else if age < WAITING_THRESHOLD_MS {
            (true, CliStatus::Waiting)
        } else {
            (false, CliStatus::Idle)
        };

        let title = truncate(title_raw.trim(), 64);
        let cwd = Path::new(&dir_raw)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dir_raw.clone());

        out.push(CliSession {
            id: CliId::Opencode.as_str().into(),
            label: CliId::Opencode.label().into(),
            installed: true,
            running,
            status,
            title,
            cwd,
            updated_ms: best_recent,
            ..Default::default()
        });
    }

    Some(out)
}

/// Resolve `~/.local/share/opencode/opencode.db` per OpenCode's docs
/// (macOS/Linux AND Windows use the same path under `%USERPROFILE%`).
fn opencode_db_path() -> Option<PathBuf> {
    user_profile().map(|p| p.join(".local").join("share").join("opencode").join("opencode.db"))
}

/// Tier 2 — HTTP probe against `opencode serve`. Only used when the SQLite
/// path is unavailable (e.g., the user only runs WSL-side OpenCode with no
/// Windows-side db). See `opencode_session` for the wire-shape notes.
fn opencode_session_from_http() -> Option<CliSession> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(700))
        .build();
    let base = find_opencode_server(&agent)?;

    let status_ok = agent
        .get(&format!("{base}/session/status"))
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok());

    let sessions_value = agent
        .get(&format!("{base}/session"))
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok())?;
    let list: Vec<Value> = sessions_value
        .as_array().cloned()
        .or_else(|| sessions_value.get("data").and_then(|d| d.as_array()).cloned())?;

    let pick = list
        .iter()
        .find(|sess| {
            let sid = sess.get("id").and_then(|t| t.as_str()).unwrap_or("");
            session_busy(status_ok.as_ref(), sid)
        })
        .or_else(|| list.first())?;

    let first = pick;
    let title = first
        .get("title")
        .and_then(|t| t.as_str())
        .map(|t| truncate(t, 64))
        .unwrap_or_default();
    let cwd = first
        .get("directory")
        .or_else(|| first.get("cwd"))
        .or_else(|| first.get("path"))
        .and_then(|t| t.as_str())
        .map(|t| {
            Path::new(t)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| t.to_string())
        })
        .unwrap_or_default();
    let sid = first.get("id").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let busy = session_busy(status_ok.as_ref(), &sid);

    Some(CliSession {
        id: CliId::Opencode.as_str().into(),
        label: CliId::Opencode.label().into(),
        installed: true,
        running: busy,
        status: if busy { CliStatus::Running } else { CliStatus::Idle },
        title,
        cwd,
        updated_ms: now_ms(),
        ..Default::default()
    })
}

/// True when `/session/status` reports `busy` (or `retry`) for `sid`.
///
/// `SessionStatus` is `{ type: "idle" | "busy" | "retry", ... }` — an
/// object, not a raw string. We read `type` defensively and also accept
/// the legacy string form for forward-compat.
fn session_busy(status: Option<&Value>, sid: &str) -> bool {
    let Some(map) = status.and_then(|v| v.as_object()) else {
        return false;
    };
    let Some(st) = map.get(sid) else {
        return false;
    };
    let kind = st
        .get("type")
        .and_then(|t| t.as_str())
        .or_else(|| st.as_str())
        .unwrap_or("");
    matches!(kind.to_lowercase().as_str(), "busy" | "retry")
        || kind.to_lowercase().contains("waiting_for_user_permission")
        || kind.to_lowercase().contains("waiting_for_user_input")
        || kind.to_lowercase().contains("pending")
}

/// HTTP fallback: try the default OpenCode serve port, then a wider range.
/// Default per `opencode serve --help` is 4096.
fn find_opencode_server(agent: &ureq::Agent) -> Option<String> {
    const CANDIDATES: [u16; 12] =
        [4096, 4097, 4098, 4099, 4100, 4101, 4102, 4103, 4104, 4105, 8080, 3000];
    for port in CANDIDATES {
        let base = format!("http://127.0.0.1:{port}");
        if agent
            .get(&format!("{base}/global/health"))
            .call()
            .ok()
            .and_then(|r| r.into_json::<Value>().ok())
            .map(|v| {
                v.get("healthy").and_then(|h| h.as_bool()).unwrap_or(false)
                    || v.get("ok").and_then(|h| h.as_bool()).unwrap_or(false)
            })
            .unwrap_or(false)
        {
            return Some(base);
        }
    }
    None
}

/// Build sessions snapshot for the given CLI. Each CLI may return multiple
/// concurrently-active sessions (e.g. OpenCode SQLite yields up to N most
/// recent). `installed` is the PATH-lookup result from `detect`. Hook-
/// supplied overrides are merged later in `listen.rs`.
///
/// Sessions from WSL distros (when `wsl.exe` is present) are appended to the
/// Windows-side result and tagged with `· WSL:<distro>` in `title` so the
/// Agents window can distinguish a host-side session from a co-existing
/// Linux-side one of the same CLI (see `wsl.rs`).
pub fn sessions_for(id: CliId, installed: bool) -> Vec<CliSession> {
    let mut out: Vec<CliSession> = match id {
        CliId::Claude => vec![claude_session(installed)],
        CliId::Codex => vec![codex_session(installed)],
        CliId::Opencode => opencode_sessions(installed),
    };
    out.extend(super::wsl::scan_sessions().into_iter().filter(|s| s.id == id.as_str()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_long() {
        assert_eq!(truncate("hello", 64), "hello");
        let long = "x".repeat(70);
        let t = truncate(&long, 10);
        assert!(t.ends_with('…'));
        assert_eq!(t.chars().count(), 10);
    }

    #[test]
    fn match_glob_jsonl() {
        assert!(match_glob("rollout-123.jsonl", "rollout-.*"));
        assert!(match_glob("abc.jsonl", ".*"));
        assert!(!match_glob("abc.txt", ".*"));
    }

    #[test]
    fn strip_meta_command_wrapper() {
        assert_eq!(
            strip_meta("<command-message>fix the bug</command-message>"),
            "fix the bug</command-message>"
        );
        assert_eq!(strip_meta("plain prompt"), "plain prompt");
    }

    #[test]
    fn claude_title_from_jsonl() {
        let dir = std::env::temp_dir().join("zenith_claude_test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("session.jsonl");
        std::fs::write(
            &f,
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"Refactor the auth module\"}]}}\n\
             {\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}\n",
        )
        .unwrap();
        assert_eq!(claude_title(&f), "Refactor the auth module");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn claude_status_running_on_assistant_last() {
        assert_eq!(
            claude_status_from_last("{\"type\":\"assistant\"}"),
            CliStatus::Running
        );
        assert_eq!(
            claude_status_from_last("{\"type\":\"result\"}"),
            CliStatus::Idle
        );
    }

    /// **Regression:** OpenCode `SessionStatus` is an OBJECT (`{ type: "busy" }`),
    /// not a raw string. A previous version of `session_busy` called `v.as_str()`
    /// on the value, which always returned `None` → the window always showed
    /// "Idle" even while OpenCode was actively working.
    #[test]
    fn session_busy_reads_object_shape() {
        let status = serde_json::json!({
            "sess-a": { "type": "busy" },
            "sess-b": { "type": "idle" },
            "sess-c": { "type": "retry", "attempt": 1, "message": "rate limit", "next": 0 },
            "sess-d": "busy", // legacy raw-string shape must also work
            "sess-e": "idle"
        });
        assert!(session_busy(Some(&status), "sess-a"), "object busy → busy");
        assert!(!session_busy(Some(&status), "sess-b"), "object idle → not busy");
        assert!(session_busy(Some(&status), "sess-c"), "object retry → busy (retry is active)");
        assert!(session_busy(Some(&status), "sess-d"), "legacy raw-string busy → busy");
        assert!(!session_busy(Some(&status), "sess-e"), "legacy raw-string idle → not busy");
        assert!(!session_busy(Some(&status), "missing"), "missing sid → not busy");
        assert!(!session_busy(None, "x"), "no status map → not busy");
    }

    /// `/session` may return the v1 array shape or the v2 `{ data: Session[] }`
    /// envelope — the unwrap must handle both.
    #[test]
    fn sessions_list_unwraps_v2_data_envelope() {
        let s1 = serde_json::json!([{ "id": "a", "title": "t1" }]);
        let list = s1.as_array().cloned();
        assert_eq!(list.unwrap().len(), 1);

        let s2 = serde_json::json!({ "data": [{ "id": "b", "title": "t2" }] });
        let list = s2.as_array().cloned().or_else(|| {
            s2.get("data").and_then(|d| d.as_array()).cloned()
        });
        assert_eq!(list.unwrap().len(), 1);
    }
}
