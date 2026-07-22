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

/// Refresh the opencode plugin on startup if already installed.
/// Regenerates with the current bridge port so the plugin doesn't hold a stale URL.
pub fn refresh_opencode_plugin() {
    let path = opencode_plugin_path();
    let config_path = opencode_config_path();
    eprintln!("[zenith:ai-cli:hook] refresh_opencode_plugin: plugin_path={:?} exists={}", path, path.exists());
    eprintln!("[zenith:ai-cli:hook]   config_path={:?} exists={}", config_path, config_path.exists());
    if path.exists() {
        if let Err(e) = install_opencode_plugin() {
            eprintln!("[zenith:ai-cli:hook] failed to refresh opencode plugin: {e}");
        }
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

// ── opencode plugin (JS) ──────────────────────────────────────────
// Uses opencode's plugin API (https://opencode.ai/docs/plugins).
// Plugin file lives at ~/.config/opencode/plugins/zenith-ai-cli-bridge.js
// and is registered in opencode.jsonc under the "plugin" key.

fn opencode_config_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(".config")
        .join("opencode")
}

fn opencode_config_path() -> PathBuf {
    opencode_config_dir().join("opencode.jsonc")
}

fn opencode_plugins_dir() -> PathBuf {
    opencode_config_dir().join("plugins")
}

fn opencode_plugin_path() -> PathBuf {
    opencode_plugins_dir().join("zenith-ai-cli-bridge.js")
}

fn bridge_base_url() -> String {
    let port = crate::ai_cli::server::bridge_port().unwrap_or(4099);
    format!("http://127.0.0.1:{port}")
}

/// Install (or refresh) the opencode plugin and register it in opencode.jsonc.
/// Always regenerates the plugin JS so the bridge port is current.
fn install_opencode_plugin() -> Result<bool, String> {
    let dir = opencode_plugins_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir plugins: {e}"))?;

    let base_url = bridge_base_url();
    let plugin_code = format!(r#"// Auto-installed by Zenith ai-cli widget
export default async () => {{
  const BASE = "{base_url}";
  const post = (event, extra = {{}}) => {{
    fetch(BASE + "/ai-cli/event", {{
      method: "POST",
      headers: {{ "Content-Type": "application/json" }},
      body: JSON.stringify({{ cli: "opencode", event, ...extra, timestamp_ms: Date.now() }}),
    }}).catch(() => {{}});
  }};
  return {{
    event: async ({{ event }}) => {{
      const t = event.type;
      const p = event.properties || {{}};
      if (t === "session.created" && p.info) {{
        post("started", {{ session_id: p.info.id, prompt_label: p.info.title }});
      }}
      if (t === "session.status" && p.sessionID) {{
        if (p.status?.type === "busy") post("started", {{ session_id: p.sessionID }});
        else if (p.status?.type === "idle") post("idle", {{ session_id: p.sessionID }});
      }}
      if (t === "session.idle" && p.sessionID) {{
        post("idle", {{ session_id: p.sessionID }});
      }}
      if (t === "session.error") {{
        const msg = p.error?.data?.message || p.error?.message || "Unknown error";
        post("failed", {{ session_id: p.sessionID || "", error_message: msg }});
      }}
      if (t === "session.deleted" && p.info) {{
        post("completed", {{ session_id: p.info.id }});
      }}
      // Permission request — waiting for user confirmation
      if (t === "permission.updated" && p.id) {{
        post("waiting", {{ session_id: p.sessionID, prompt_label: p.title || p.type }});
      }}
      // Permission resolved — back to busy/idle
      if (t === "permission.replied" && p.sessionID) {{
        post("started", {{ session_id: p.sessionID }});
      }}
      // Question asked — waiting for user answer
      if (t === "question.asked" && p.id) {{
        post("waiting", {{ session_id: p.sessionID, prompt_label: "question" }});
      }}
    }},
    "shell.env": async (_, output) => {{
      output.env.ZENITH_AI_CLI_ACTIVE = "1";
    }},
  }};
}};
"#);

    let path = opencode_plugin_path();
    std::fs::write(&path, &plugin_code).map_err(|e| format!("write plugin: {e}"))?;
    eprintln!("[zenith:ai-cli:hook] plugin written: {} ({} bytes)", path.display(), plugin_code.len());

    register_plugin_in_config()?;

    // Clean up old location (v1 plugin at %APPDATA%/opencode/plugins/)
    let old_path = old_opencode_plugin_path();
    if old_path.exists() {
        let _ = std::fs::remove_file(&old_path);
        eprintln!("[zenith:ai-cli:hook] cleaned up old plugin at {}", old_path.display());
    }

    Ok(true)
}

fn register_plugin_in_config() -> Result<(), String> {
    let config_path = opencode_config_path();
    let plugin_ref = format!("file:///{}", opencode_plugin_path().to_string_lossy().replace('\\', "/"));

    eprintln!("[zenith:ai-cli:hook] register_plugin_in_config: path={:?} ref={}", config_path, plugin_ref);

    let mut cfg: serde_json::Value = if config_path.exists() {
        let raw = std::fs::read_to_string(&config_path).map_err(|e| format!("read config: {e}"))?;
        eprintln!("[zenith:ai-cli:hook]   existing config: {raw}");
        serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    } else {
        eprintln!("[zenith:ai-cli:hook]   config file does not exist, creating");
        serde_json::Value::Object(serde_json::Map::new())
    };

    if !cfg.is_object() {
        eprintln!("[zenith:ai-cli:hook]   config was not an object, resetting");
        cfg = serde_json::Value::Object(serde_json::Map::new());
    }

    let map = cfg.as_object_mut().unwrap();
    let plugins = map
        .entry("plugin")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let arr = plugins.as_array_mut().unwrap();

    eprintln!("[zenith:ai-cli:hook]   current plugin array (before): {:?}", arr);

    // Remove stale reference to our plugin, then add current one
    arr.retain(|p| {
        p.as_str()
            .map(|s| !s.contains("zenith-ai-cli-bridge"))
            .unwrap_or(true)
    });
    arr.push(serde_json::Value::String(plugin_ref));

    eprintln!("[zenith:ai-cli:hook]   plugin array (after): {:?}", arr);

    // Write back — use JSON even though the extension is .jsonc (opencode accepts both)
    let raw = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, &raw).map_err(|e| format!("write config: {e}"))?;
    eprintln!("[zenith:ai-cli:hook] config written ({} bytes)", raw.len());

    Ok(())
}

fn uninstall_opencode_plugin() -> Result<bool, String> {
    let mut changed = false;

    // Remove plugin file
    let path = opencode_plugin_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove plugin: {e}"))?;
        eprintln!("[zenith:ai-cli] opencode plugin file removed");
        changed = true;
    }

    // Remove from opencode.jsonc
    let config_path = opencode_config_path();
    if config_path.exists() {
        let raw = std::fs::read_to_string(&config_path).map_err(|e| format!("read config: {e}"))?;
        if let Ok(mut cfg) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(map) = cfg.as_object_mut() {
                if let Some(plugins) = map.get_mut("plugin").and_then(|p| p.as_array_mut()) {
                    let before = plugins.len();
                    plugins.retain(|p| {
                        p.as_str()
                            .map(|s| !s.contains("zenith-ai-cli-bridge"))
                            .unwrap_or(true)
                    });
                    if plugins.len() != before {
                        if plugins.is_empty() {
                            map.remove("plugin");
                        }
                        let raw = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
                        std::fs::write(&config_path, raw).map_err(|e| format!("write config: {e}"))?;
                        eprintln!("[zenith:ai-cli] opencode plugin unregistered from config");
                        changed = true;
                    }
                }
            }
        }
    }

    // Clean up old location
    let old_path = old_opencode_plugin_path();
    if old_path.exists() {
        let _ = std::fs::remove_file(&old_path);
    }

    Ok(changed)
}

fn old_opencode_plugin_path() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("opencode")
        .join("plugins")
        .join("zenith-ai-cli-bridge.ts")
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
