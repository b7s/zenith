//! Git Manager widget domain.
//!
//! Aggregates failed-CI / open-PR / repo-summary signals across the
//! user's GitHub, GitLab, and Bitbucket accounts. Per-account auth tokens
//! are stored in `config.json` under `widgets.config.git.accounts` as
//! DPAPI-protected base64 blobs (see `secrets.rs`).
//!
//! Architecture:
//!   - `mod.rs`         — DTO types mirrored in `shared/types.ts`.
//!   - `secrets.rs`     — DPAPI `CryptProtectData` / `CryptUnprotectData`.
//!   - `provider/*`      — pure HTTPS clients returning unified `AcctInventory`.
//!   - `poll.rs`         — background sequential per-account fan-out, `Mutex<GitState>`
//!                         cache, emits `zenith:git-changed` on totals change.
//!   - `commands.rs`    — thin `#[tauri::command]` adapters + window opener
//!                         (calendar-popup shape, see §13.14 monitor clamping).

pub mod commands;
pub mod listen;
pub mod model;
pub mod provider;
pub mod secrets;
