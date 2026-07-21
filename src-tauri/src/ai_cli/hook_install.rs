//! Install/uninstall hook entries into Claude Code and Codex config files.
//! Idempotent — checks existing settings and merges, keeping user hooks intact.

use std::path::PathBuf;

use super::model::CliId;

/// Install the hook entry for a given CLI into its user-level settings file.
/// Returns `true` if hooks were installed/changed, `false` if already present.
pub fn install(cli: CliId) -> Result<bool, String> {
    match cli {
        CliId::Claude => install_claude_hooks(),
        CliId::Codex => install_codex_hooks(),
        CliId::Opencode => install_opencode_plugin(),
    }
}

/// Uninstall our hook entries (leaving user hooks intact).
pub fn uninstall(cli: CliId) -> Result<bool, String> {
    match cli {
        CliId::Claude => uninstall_claude_hooks(),
        CliId::Codex => uninstall_codex_hooks(),
        CliId::Opencode => uninstall_opencode_plugin(),
    }
}

fn bridge_endpoint() -> String {
    let port = crate::ai_cli::server::bridge_port().unwrap_or(4099);
    format!("http://127.0.0.1:{port}/ai-cli/event")
}

// ── Claude Code hooks ──────────────────────────────────────────────

fn claude_settings_path() -> PathBuf {
    let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    home.join(".claude").join("settings.json")
}

fn install_claude_hooks() -> Result<bool, String> {
    let path = claude_settings_path();
    let mut settings: serde_json::Value = if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("read settings: {e}"))?;
        serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    let bridge = bridge_endpoint();
    // Build the hook entries using PowerShell for Windows
    let hook_script = format!(
        "powershell.exe -NoProfile -NonInteractive -Command \"$b=Invoke-RestMethod -Uri '{bridge}' -Method POST -Body (@{{ cli='claude';event='SESSION_START' }} | ConvertTo-Json) -ContentType 'application/json' 2>$null; $b\""
    );

    if !settings.is_object() {
        return Err("settings root not an object".into());
    }
    if settings.get("hooks").is_none() {
        let map = settings.as_object_mut().ok_or("settings not an object")?;
        map.insert("hooks".into(), serde_json::Value::Object(serde_json::Map::new()));
    }
    let hooks = settings
        .get_mut("hooks")
        .and_then(|v| v.as_object_mut())
        .ok_or("hooks not an object")?;

    let changed = insert_hook_entry(hooks, "SessionStart", &hook_script, "zenith-ai-cli")
        | insert_hook_entry(hooks, "Stop", &hook_script.replace("SESSION_START", "STOP"), "zenith-ai-cli")
        | insert_hook_entry(hooks, "StopFailure", &hook_script.replace("SESSION_START", "STOP_FAILURE"), "zenith-ai-cli");

    if changed {
        let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        std::fs::write(&path, raw).map_err(|e| format!("write settings: {e}"))?;
    }

    Ok(changed)
}

fn uninstall_claude_hooks() -> Result<bool, String> {
    let path = claude_settings_path();
    if !path.exists() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("read: {e}"))?;
    let mut settings: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let mut changed = false;
    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for event in ["SessionStart", "Stop", "StopFailure"] {
            if let Some(arr) = hooks.get_mut(event).and_then(|a| a.as_array_mut()) {
                let before = arr.len();
                arr.retain(|h| {
                    h.get("command").and_then(|c| c.as_str())
                        .map(|c| !c.contains("zenith-ai-cli"))
                        .unwrap_or(true)
                });
                if arr.len() != before {
                    changed = true;
                }
            }
        }
    }
    if changed {
        let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        std::fs::write(&path, raw).map_err(|e| format!("write: {e}"))?;
    }
    Ok(changed)
}

// ── Codex hooks ────────────────────────────────────────────────────

fn codex_hooks_path() -> PathBuf {
    let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    home.join(".codex").join("hooks.json")
}

fn install_codex_hooks() -> Result<bool, String> {
    let path = codex_hooks_path();
    let mut hooks: serde_json::Value = if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("read hooks: {e}"))?;
        serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    let bridge = bridge_endpoint();
    let hook_script = format!(
        "powershell.exe -NoProfile -NonInteractive -Command \"$b=Invoke-RestMethod -Uri '{bridge}' -Method POST -Body (@{{ cli='codex';event='SESSION_START' }} | ConvertTo-Json) -ContentType 'application/json' 2>$null; $b\""
    );

    let hooks_map = hooks.as_object_mut().ok_or("hooks not object")?;
    let changed = insert_hook_entry(hooks_map, "SessionStart", &hook_script, "zenith-ai-cli")
        | insert_hook_entry(hooks_map, "Stop", &hook_script.replace("SESSION_START", "STOP"), "zenith-ai-cli")
        | insert_hook_entry(hooks_map, "StopFailure", &hook_script.replace("SESSION_START", "STOP_FAILURE"), "zenith-ai-cli");

    if changed {
        let raw = serde_json::to_string_pretty(&hooks).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        std::fs::write(&path, raw).map_err(|e| format!("write hooks: {e}"))?;
    }

    Ok(changed)
}

fn uninstall_codex_hooks() -> Result<bool, String> {
    let path = codex_hooks_path();
    if !path.exists() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("read: {e}"))?;
    let mut hooks: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let mut changed = false;
    if let Some(hooks_map) = hooks.as_object_mut() {
        for event in ["SessionStart", "Stop", "StopFailure"] {
            if let Some(arr) = hooks_map.get_mut(event).and_then(|a| a.as_array_mut()) {
                let before = arr.len();
                arr.retain(|h| {
                    h.get("command").and_then(|c| c.as_str())
                        .map(|c| !c.contains("zenith-ai-cli"))
                        .unwrap_or(true)
                });
                if arr.len() != before {
                    changed = true;
                }
            }
        }
    }
    if changed {
        let raw = serde_json::to_string_pretty(&hooks).map_err(|e| e.to_string())?;
        std::fs::write(&path, raw).map_err(|e| format!("write: {e}"))?;
    }
    Ok(changed)
}

// ── opencode plugin ───────────────────────────────────────────────

fn opencode_plugins_dir() -> PathBuf {
    std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("opencode")
        .join("plugins")
}

fn install_opencode_plugin() -> Result<bool, String> {
    let dir = opencode_plugins_dir();
    let path = dir.join("zenith-ai-cli-bridge.ts");
    if path.exists() {
        return Ok(false); // already installed
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let bridge = bridge_endpoint();
    let plugin_code = format!(r#"// Auto-installed by Zenith ai-cli widget
export const ZenithAiCliBridge = async (ctx) => {{
  const bridgeUrl = "{bridge}";
  const post = (event, extra) => {{
    try {{
      fetch(bridgeUrl, {{
        method: "POST",
        headers: {{ "Content-Type": "application/json" }},
        body: JSON.stringify({{ cli: "opencode", event, ...extra, timestamp_ms: Date.now() }}),
      }}).catch(() => {{}});
    }} catch(e) {{}}
  }};
  return {{
    "session.idle": async (input, output) => post("IDLE", {{ prompt_label: input?.title }}),
    "session.error": async (input, output) => post("FAILED", {{
      error_message: input?.error || input?.message,
      prompt_label: input?.title,
    }}),
    "session.created": async (input, output) => post("STARTED", {{ prompt_label: input?.title }}),
  }};
}};
"#);
    std::fs::write(&path, &plugin_code).map_err(|e| format!("write plugin: {e}"))?;
    eprintln!("[zenith:ai-cli] opencode plugin installed: {}", path.display());
    Ok(true)
}

fn uninstall_opencode_plugin() -> Result<bool, String> {
    let path = opencode_plugins_dir().join("zenith-ai-cli-bridge.ts");
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove plugin: {e}"))?;
        eprintln!("[zenith:ai-cli] opencode plugin removed");
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── shared helpers ────────────────────────────────────────────────

/// Insert a hook entry into a hooks map for a given event, keyed by a
/// comment/command pattern so we can idempotently identify our own entries.
fn insert_hook_entry(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    event: &str,
    command: &str,
    marker: &str,
) -> bool {
    let entry = hooks
        .entry(event.to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let arr = entry.as_array_mut().expect("hook entry must be array");

    // Check if any existing hook already contains our marker
    let already = arr.iter().any(|h| {
        h.get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.contains(marker))
            .unwrap_or(false)
    });

    if already {
        return false;
    }

    // Append a new matcher-less hook entry
    let hook = serde_json::json!({
        "type": "command",
        "command": command,
        "command_marker": marker,
    });
    arr.push(hook);
    true
}
