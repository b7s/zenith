//! Calendar-sync domain: connect Google / Outlook calendars over OAuth 2.0
//! (PKCE public client) and sync their events into the shared events
//! store that the alarms widget + calendar popup already consume.
//!
//! Module layout:
//! * `model`      — `CalendarAccount`, `PendingAuth` types.
//! * `credentials`— public client ids / endpoints (placeholders).
//! * `oauth`      — PKCE flow, loopback callback, token exchange.
//! * `accounts`   — account CRUD in `config.json` (DPAPI-wrapped tokens).
//! * `provider`   — per-backend fetch + event mapping (google/outlook).
//! * `sync`       — one-shot account sync.
//! * `poll`       — background periodic sync thread.
//! * `commands`   — Tauri command adapters.
//! * `iso8601`    — std-only RFC3339 <-> Unix helpers.

pub mod accounts;
pub mod commands;
pub mod credentials;
pub mod iso8601;
pub mod model;
pub mod oauth;
pub mod poll;
pub mod provider;
pub mod sync;
