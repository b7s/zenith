//! Usage data models — mirrored in `src/shared/types.ts`.
//!
//! One `DailyUsage` per (day, cli) pair; `MonthlyUsage` aggregates them.

use serde::{Deserialize, Serialize};

/// One day's aggregated usage for one (CLI, model) pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    /// `"2026-07-24"` — local calendar day.
    pub day: String,
    /// `"claude"` | `"codex"` | `"opencode"`.
    pub cli_id: String,
    /// `"[providerID] modelID"`, e.g. `"[nvidia] z-ai/glm-5.2"`, `"[opencode] deepseek-v4-flash-free"`.
    pub model_name: String,
    pub sessions: u32,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tokens_cache_read: u64,
    pub tokens_cache_write: u64,
    /// Dollar cost (USD). For OpenCode this is the stored `cost` column;
    /// for Claude/Codex it's computed via `pricing::cost_usd`.
    pub cost_usd: f64,
}

/// Per-CLI summary for the month.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliUsageSummary {
    pub cli_id: String,
    pub sessions: u32,
    pub total_tokens_input: u64,
    pub total_tokens_output: u64,
    pub total_tokens_cache_read: u64,
    pub total_cost_usd: f64,
}

/// Full month aggregation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonthlyUsage {
    /// `"2026-07"`.
    pub month: String,
    pub by_cli: Vec<CliUsageSummary>,
    /// Sorted ascending by `day`.
    pub daily: Vec<DailyUsage>,
}

impl MonthlyUsage {
    #[allow(dead_code)]
    pub fn total_tokens(&self) -> u64 {
        self.daily
            .iter()
            .map(|d| d.tokens_input + d.tokens_output + d.tokens_cache_read)
            .sum()
    }

    #[allow(dead_code)]
    pub fn total_cost(&self) -> f64 {
        self.daily.iter().map(|d| d.cost_usd).sum()
    }
}
