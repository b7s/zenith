//! AI CLI widget domain.
//!
//! Monitors AI coding CLI sessions (opencode, claude code, codex) and
//! surfaces their status as dots on the bar widget.
//!
//! Architecture:
//!   - `model.rs`        — DTOs mirrored in `shared/types.ts`.
//!   - `detect.rs`       — PURE: scan filesystem + PATH for CLI installations.
//!   - `aggregator.rs`   — PURE: ingest events → pre-compute three-boolean dot state.
//!   - `server.rs`       — localhost HTTP endpoint for hook POSTs from sidecar + JS plugin.
//!   - `opencode_client.rs` — process-detection fallback for opencode.exe.
//!   - `hook_install.rs` — install/uninstall hooks in claude/codex/opencode config.
//!   - `commands.rs`     — thin `#[tauri::command]` adapters.
//!
//! opencode detection:
//!   1. JS plugin at ~/.config/opencode/plugins/zenith-ai-cli-bridge.js uses
//!      opencode's plugin API (`event` hook) and POSTs session events to our
//!      bridge server.
//!   2. `opencode_client.rs` polls for opencode.exe as a fallback when the
//!      plugin isn't loaded.

pub mod aggregator;
pub mod commands;
pub mod detect;
pub mod hook_install;
pub mod model;
pub mod opencode_client;
pub mod server;

pub use commands::aggregator;
