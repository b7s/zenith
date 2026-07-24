//! Managed hook installation for agent CLIs.
//!
//! Precise `waiting` (permission_prompt) and `failed` (StopFailure/error)
//! states come from lifecycle events the agents fire via hooks. Zenith
//! installs *managed* hook entries into each agent's config and removes
//! **only** those entries on uninstall — the user's own hooks are preserved.
//!
//! - **Claude Code**: `%USERPROFILE%\.claude\settings.json` — `type:"http"`
//!   hooks POST to the embedded listener. Idempotent via a URL marker.
//! - **Codex**: `%USERPROFILE%\.codex\config.toml` — command hook invoking
//!   `curl.exe` (ships with Windows 10+) which POSTs the payload. Managed
//!   entries carry a marker command path.
//! - **OpenCode**: no hooks — auto-detected via its HTTP server. `status()`
//!   reports "auto-detected".
//!
//! Fail-open: if Zenith is not running, agent hook POSTs fail silently and
//! never block the agent (Claude's HTTP hooks are non-blocking; `curl` exit
//! codes are ignored by default).

use std::path::PathBuf;

use super::model::CliId;
use super::server::HOOK_PORT;
use crate::aicli::model::AicliHookStatus;

const MARKER: &str = "zenith-aicli";

/// Base URL delivered to agents — includes the source so the listener can
/// classify without inspecting the body.
fn hook_url(source: &str) -> String {
    format!("http://127.0.0.1:{HOOK_PORT}/hook?src={source}")
}

fn user_profile() -> Option<PathBuf> {
    std::env::var("USERPROFILE").ok().map(PathBuf::from).filter(|p| p.exists())
}

fn claude_settings_path() -> Option<PathBuf> {
    user_profile().map(|p| p.join(".claude").join("settings.json"))
}

fn codex_config_path() -> Option<PathBuf> {
    user_profile().map(|p| p.join(".codex").join("config.toml"))
}

/// Current managed-hook status for every hook-capable CLI.
pub fn status() -> Vec<AicliHookStatus> {
    let mut out = Vec::new();
    out.push(claude_status());
    out.push(codex_status());
    // OpenCode needs no hooks.
    out.push(AicliHookStatus {
        id: CliId::Opencode.as_str().into(),
        installed: true,
        detail: "auto-detected".into(),
    });
    out
}

fn claude_status() -> AicliHookStatus {
    let Some(path) = claude_settings_path() else {
        return AicliHookStatus { id: "claude".into(), installed: false, detail: "no settings".into() };
    };
    let installed = std::fs::read_to_string(&path)
        .map(|s| s.contains(MARKER))
        .unwrap_or(false);
    AicliHookStatus {
        id: "claude".into(),
        installed,
        detail: if installed { "managed".into() } else { "not installed".into() },
    }
}

fn codex_status() -> AicliHookStatus {
    let Some(path) = codex_config_path() else {
        return AicliHookStatus { id: "codex".into(), installed: false, detail: "no config".into() };
    };
    let installed = std::fs::read_to_string(&path)
        .map(|s| s.contains(MARKER))
        .unwrap_or(false);
    AicliHookStatus {
        id: "codex".into(),
        installed,
        detail: if installed { "managed".into() } else { "not installed".into() },
    }
}

/// Install managed hooks for the given CLI.
pub fn install(id: CliId) -> Result<(), String> {
    match id {
        CliId::Claude => install_claude(),
        CliId::Codex => install_codex(),
        CliId::Opencode => Err("OpenCode needs no hooks (auto-detected)".into()),
    }
}

/// Remove managed hooks for the given CLI (preserving user hooks).
pub fn uninstall(id: CliId) -> Result<(), String> {
    match id {
        CliId::Claude => uninstall_claude(),
        CliId::Codex => uninstall_codex(),
        CliId::Opencode => Err("OpenCode has no managed hooks".into()),
    }
}

// ── Claude Code (.json) ──────────────────────────────────────────────────

/// Events we forward to the listener. Permission prompts + stop/failure give
/// the precise `waiting`/`failed` states.
const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "StopFailure",
    "Notification",
];

fn install_claude() -> Result<(), String> {
    let path = claude_settings_path().ok_or("no .claude dir")?;
    let mut root = read_json(&path).unwrap_or(serde_json::json!({}));
    if !root.is_object() {
        root = serde_json::json!({});
    }

    // Ensure hooks.<event> arrays exist with our managed marker handlers.
    let hooks = root
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let hooks_obj = hooks.as_object_mut().ok_or("hooks not an object")?;

    for ev in CLAUDE_EVENTS {
        let arr = hooks_obj
            .entry(*ev)
            .or_insert_with(|| serde_json::json!([]));
        let arr = arr.as_array_mut().ok_or("hook entry not an array")?;
        // Remove any pre-existing managed entry (idempotency).
        arr.retain(|h| !is_managed_claude(h));
        arr.push(serde_json::json!({
            "hooks": [{
                "type": "http",
                "url": hook_url("claude"),
                "MARKER": MARKER,
            }]
        }));
    }

    write_json_atomic(&path, &root)?;
    Ok(())
}

fn uninstall_claude() -> Result<(), String> {
    let path = claude_settings_path().ok_or("no .claude dir")?;
    let mut root = read_json(&path).unwrap_or(serde_json::json!({}));
    if let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for ev in CLAUDE_EVENTS {
            if let Some(arr) = hooks.get_mut(*ev).and_then(|h| h.as_array_mut()) {
                arr.retain(|h| !is_managed_claude(h));
            }
        }
    }
    write_json_atomic(&path, &root)?;
    Ok(())
}

/// A Claude handler is "managed" when any nested `http` hook carries our marker.
fn is_managed_claude(handler: &serde_json::Value) -> bool {
    handler
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|inner| inner.iter().any(|h| h.get("MARKER").and_then(|m| m.as_str()) == Some(MARKER)))
        .unwrap_or(false)
}

// ── Codex (.toml) ────────────────────────────────────────────────────────

/// Codex forwards lifecycle events via TOML `[hooks]` table entries. We add a
/// command hook that pipes the payload to the listener through `curl.exe`.
fn install_codex() -> Result<(), String> {
    let path = codex_config_path().ok_or("no .codex dir")?;
    let mut doc = read_toml(&path).unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));
    let table = doc
        .as_table_mut()
        .expect("toml root is a table");

    // `[hooks]` table.
    let hooks = table
        .entry("hooks")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or("hooks not a table")?;

    let url = hook_url("codex");
    let curl_cmd = format!(
        r#"curl.exe -s -X POST "{url}" -H "content-type: application/json" --data-binary @- >nul 2>&1"#
    );

    // Managed codex hook entry carries the marker in its command string.
    let entry = toml::Value::String(curl_cmd);
    let arr = hooks
        .entry(MARKER)
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or("codex hook entry not an array")?;
    arr.retain(|e| !is_managed_codex(e));
    arr.push(entry);

    write_toml_atomic(&path, &doc)?;
    Ok(())
}

fn uninstall_codex() -> Result<(), String> {
    let path = codex_config_path().ok_or("no .codex dir")?;
    let mut doc = read_toml(&path).unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));
    if let Some(hooks) = doc
        .as_table_mut()
        .and_then(|t| t.get_mut("hooks"))
        .and_then(|h| h.as_table_mut())
    {
        if let Some(arr) = hooks.get_mut(MARKER).and_then(|h| h.as_array_mut()) {
            arr.retain(|e| !is_managed_codex(e));
        }
    }
    write_toml_atomic(&path, &doc)?;
    Ok(())
}

fn is_managed_codex(v: &toml::Value) -> bool {
    v.as_str().map(|s| s.contains(MARKER)).unwrap_or(false)
}

// ── IO helpers ───────────────────────────────────────────────────────────

fn read_json(path: &PathBuf) -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_json_atomic(path: &PathBuf, value: &serde_json::Value) -> Result<(), String> {
    let parent = path.parent().ok_or("no parent dir")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

fn read_toml(path: &PathBuf) -> Option<toml::Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    toml::from_str(&raw).ok()
}

fn write_toml_atomic(path: &PathBuf, value: &toml::Value) -> Result<(), String> {
    let parent = path.parent().ok_or("no parent dir")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    let text = toml::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}
