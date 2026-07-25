//! AI coding-agent status domain.
//!
//! Detects installed AI CLI agents (Claude Code, Codex, OpenCode), tracks
//! per-session status (running / waiting / failed) via a blend of process
//! enumeration, transcript scanning, the OpenCode HTTP server, and managed
//! hook events delivered to the embedded HTTP listener.
//!
//! `listen::spawn` owns the single `zenith:aicli-changed` emitter (mirrors
//! the git/workspace listener contract — never emit from commands).

pub mod commands;
pub mod detect;
pub mod hooks;
pub mod listen;
pub mod model;
pub mod pricing;
pub mod scan;
pub mod server;
pub mod usage;
pub mod usage_model;
pub mod wsl;
