//! AI CLI widget domain.
//!
//! Monitors AI coding CLI sessions (opencode, claude code, codex) and
//! surfaces their status as dots on the bar widget.
//!
//! Architecture:
//!   - `model.rs`        — DTOs mirrored in `shared/types.ts`.
//!   - `detect.rs`       — PURE: scan filesystem + PATH for CLI installations.
//!   - `aggregator.rs`   — PURE: ingest events → pre-compute three-boolean dot state.
//!   - `server.rs`       — localhost HTTP endpoint for hook POSTs from sidecar.
//!   - `opencode_client.rs` — SSE / HTTP client for opencode's server.
//!   - `hook_install.rs` — install/uninstall hooks in claude/codex config.
//!   - `commands.rs`     — thin `#[tauri::command]` adapters.

pub mod aggregator;
pub mod commands;
pub mod detect;
pub mod hook_install;
pub mod model;
pub mod opencode_client;
pub mod server;

pub use commands::aggregator;
