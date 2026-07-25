use serde::{Deserialize, Serialize};

/// One supported AI coding agent. Mirrored as `CliId` in `shared/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CliId {
    Claude,
    Codex,
    Opencode,
}

impl CliId {
    /// Stable string id used in config + transcripts.
    pub fn as_str(self) -> &'static str {
        match self {
            CliId::Claude => "claude",
            CliId::Codex => "codex",
            CliId::Opencode => "opencode",
        }
    }

    /// Human-readable display label.
    pub fn label(self) -> &'static str {
        match self {
            CliId::Claude => "Claude Code",
            CliId::Codex => "Codex (GPT)",
            CliId::Opencode => "OpenCode",
        }
    }

    /// The agent's executable name on PATH (no extension — resolve_bin adds
    /// PATHEXT candidates). For `claude` this resolves the npm-installed
    /// `claude.cmd` launcher; for opencode the `opencode.exe` / `opencode.cmd`.
    pub fn bin(self) -> &'static str {
        match self {
            CliId::Claude => "claude",
            CliId::Codex => "codex",
            CliId::Opencode => "opencode",
        }
    }

    pub const ALL: [CliId; 3] = [CliId::Claude, CliId::Codex, CliId::Opencode];
}

/// Per-session status. Mirrored as `CliStatus` in `shared/types.ts`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CliStatus {
    #[default]
    Idle,
    Running,
    Waiting,
    Failed,
}

/// One agent session snapshot. Mirrored as `CliSession` in `shared/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliSession {
    pub id: String,
    pub label: String,
    pub installed: bool,
    pub running: bool,
    pub status: CliStatus,
    /// First user prompt or project basename — truncated to ~60 chars.
    pub title: String,
    /// Project working-directory basename (last path segment).
    pub cwd: String,
    /// OS process id of the running agent, or 0.
    pub pid: u32,
    /// Epoch ms of the last activity seen for this session.
    pub updated_ms: i64,
}

/// Aggregate totals driving the bar status dots. Most-severe active dot wins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AicliTotals {
    pub running: u32,
    pub waiting: u32,
    pub failed: u32,
}

/// Full state snapshot. Mirrored as `AicliState` in `shared/types.ts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AicliState {
    pub sessions: Vec<CliSession>,
    pub totals: AicliTotals,
    /// Set of CLI ids the user has chosen to monitor (after first-run seeding).
    pub monitored: Vec<String>,
}

/// Per-CLI managed-hook status. Mirrored as `AicliHookStatus` in TS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AicliHookStatus {
    pub id: String,
    pub installed: bool,
    pub detail: String,
}

impl AicliState {
    /// Recompute the aggregate totals from `sessions`. A `Failed` session
    /// contributes to `failed`; `Waiting` to `waiting`; `Running` to `running`.
    pub fn recompute_totals(&mut self) {
        let mut t = AicliTotals::default();
        for s in &self.sessions {
            if !s.installed || !s.running {
                continue;
            }
            match s.status {
                CliStatus::Failed => t.failed += 1,
                CliStatus::Waiting => t.waiting += 1,
                CliStatus::Running => t.running += 1,
                CliStatus::Idle => {}
            }
        }
        self.totals = t;
    }
}
