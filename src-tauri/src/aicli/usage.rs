//! Pure usage-aggregation services — token counts + cost per CLI per day.
//!
//! Three data sources:
//! - **OpenCode**: SQLite (`opencode.db`) — `cost` stored directly on the
//!   `session` row. SQL `GROUP BY date(...)`.
//! - **Claude Code**: JSONL transcripts under `~/.claude/projects/` —
//!   `message.usage` on assistant entries; cost computed via `pricing.rs`.
//! - **Codex**: JSONL rollouts under `~/.codex/sessions/<date>/` —
//!   usage probed from OpenAI-shaped payloads; cost computed.
//!
//! No `tauri::` types (AGENTS §3). Unit-testable.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::model::CliId;
use super::pricing;
use super::usage_model::{CliUsageSummary, DailyUsage, MonthlyUsage};

type ModelUsage = Vec<(String, u64, u64, u64, u64, f64)>;

/// Return the current month in `"YYYY-MM"` format (local time best-effort).
pub fn current_month() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    month_from_epoch_ms(secs * 1000)
}

/// Convert epoch milliseconds to a `"YYYY-MM"` string.
/// Uses a lightweight UTC conversion (SQLite applies `'localtime'` for
/// OpenCode; for Claude/Codex we use file mtime which is already local).
pub fn month_from_epoch_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs / 86400;
    let (year, month) = days_to_year_month(days);
    format!("{year:04}-{month:02}")
}

/// Convert epoch milliseconds to a `"YYYY-MM-DD"` string (UTC).
pub fn day_from_epoch_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs / 86400;
    let (year, month, day) = days_to_year_month_day(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn days_to_year_month(days: i64) -> (i32, u32) {
    let (y, m, _) = days_to_year_month_day(days);
    (y, m)
}

fn days_to_year_month_day(mut days: i64) -> (i32, u32, u32) {
    // Algorithm: Howard Hinnant's "date lib" civil-from-days, simplified.
    days += 719468; // days since 0000-03-01
    let era_len = 146097i64;
    let era = if days >= 0 { days / era_len } else { (days - era_len + 1) / era_len };
    let doe = days - era * era_len; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// ── Public aggregator ──────────────────────────────────────────────
pub fn monthly_usage(month: &str) -> MonthlyUsage {
    let mut daily: Vec<DailyUsage> = Vec::new();

    daily.extend(opencode_usage(month));
    daily.extend(claude_usage(month));
    daily.extend(codex_usage(month));

    // Merge same-day same-cli entries and sort ascending by day.
    daily = merge_daily(daily);

    let by_cli = build_summary(&daily);

    MonthlyUsage {
        month: month.to_string(),
        by_cli,
        daily,
    }
}

/// Merge entries with identical (day, cli_id, model_name) by summing their fields.
fn merge_daily(mut daily: Vec<DailyUsage>) -> Vec<DailyUsage> {
    let mut map: BTreeMap<(String, String, String), DailyUsage> = BTreeMap::new();
    for d in daily.drain(..) {
        let key = (d.day.clone(), d.cli_id.clone(), d.model_name.clone());
        map.entry(key)
            .and_modify(|e| {
                e.sessions += d.sessions;
                e.tokens_input += d.tokens_input;
                e.tokens_output += d.tokens_output;
                e.tokens_cache_read += d.tokens_cache_read;
                e.tokens_cache_write += d.tokens_cache_write;
                e.cost_usd += d.cost_usd;
            })
            .or_insert(d);
    }
    let mut out: Vec<DailyUsage> = map.into_values().collect();
    out.sort_by(|a, b| {
        a.day
            .cmp(&b.day)
            .then(a.cli_id.cmp(&b.cli_id))
            .then(a.model_name.cmp(&b.model_name))
    });
    out
}

fn build_summary(daily: &[DailyUsage]) -> Vec<CliUsageSummary> {
    let mut map: BTreeMap<String, CliUsageSummary> = BTreeMap::new();
    for d in daily {
        map.entry(d.cli_id.clone())
            .and_modify(|s| {
                s.sessions += d.sessions;
                s.total_tokens_input += d.tokens_input;
                s.total_tokens_output += d.tokens_output;
                s.total_tokens_cache_read += d.tokens_cache_read;
                s.total_cost_usd += d.cost_usd;
            })
            .or_insert(CliUsageSummary {
                cli_id: d.cli_id.clone(),
                sessions: d.sessions,
                total_tokens_input: d.tokens_input,
                total_tokens_output: d.tokens_output,
                total_tokens_cache_read: d.tokens_cache_read,
                total_cost_usd: d.cost_usd,
            });
    }
    let mut out: Vec<CliUsageSummary> = map.into_values().collect();
    out.sort_by(|a, b| a.cli_id.cmp(&b.cli_id));
    out
}

// ════════════════════════════════════════════════════════════════════
// OpenCode — SQLite direct read
// ════════════════════════════════════════════════════════════════════

/// Extract model name from OpenCode session `model` JSON column.
/// Format: `{"providerID":"nvidia","id":"z-ai/glm-5.1"}` → `[nvidia] z-ai/glm-5.1`.
pub fn opencode_usage(month: &str) -> Vec<DailyUsage> {
    let Some(db) = opencode_db_path() else {
        return Vec::new();
    };
    if !db.exists() {
        return Vec::new();
    }

    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_URI
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = match rusqlite::Connection::open_with_flags(&db, flags)
        .or_else(|_| rusqlite::Connection::open(&db))
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(200));

    let sql = "\
        SELECT date(time_created/1000, 'unixepoch', 'localtime') AS day, \
               CASE WHEN model IS NOT NULL AND model != '' \
                 THEN '[' || json_extract(model, '$.providerID') || '] ' || json_extract(model, '$.id') \
                 ELSE 'unknown' \
               END AS model_name, \
               COUNT(*)                          AS sessions, \
               SUM(cost)                         AS cost, \
               SUM(tokens_input)                 AS ti, \
               SUM(tokens_output)                AS toks, \
               SUM(tokens_cache_read)            AS tcr, \
               SUM(tokens_cache_write)           AS tcw \
        FROM session \
        WHERE time_archived IS NULL \
          AND strftime('%Y-%m', time_created/1000, 'unixepoch', 'localtime') = ?1 \
        GROUP BY day, model_name ORDER BY day ASC";

    let mut out = Vec::new();
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    let rows = stmt
        .query_map(rusqlite::params![month], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        });
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            let (day, model_name, sessions, cost, ti, toks, tcr, tcw) = row;
            out.push(DailyUsage {
                day,
                cli_id: CliId::Opencode.as_str().into(),
                model_name,
                sessions: sessions as u32,
                tokens_input: ti as u64,
                tokens_output: toks as u64,
                tokens_cache_read: tcr as u64,
                tokens_cache_write: tcw as u64,
                cost_usd: cost,
            });
        }
    }
    out
}

fn opencode_db_path() -> Option<PathBuf> {
    user_profile().map(|p| {
        p.join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db")
    })
}

// ════════════════════════════════════════════════════════════════════
// Claude Code — JSONL transcript walk
// ════════════════════════════════════════════════════════════════════

pub fn claude_usage(month: &str) -> Vec<DailyUsage> {
    let Some(profile) = user_profile() else {
        return Vec::new();
    };
    let projects = profile.join(".claude").join("projects");
    if !projects.exists() {
        return Vec::new();
    }

    // (day, model) -> DailyUsage (accumulates across all transcripts for that day+model)
    let mut by_day_model: BTreeMap<(String, String), DailyUsage> = BTreeMap::new();

    walk_files(&projects, &mut |path| {
        if !is_jsonl(path) {
            return;
        }
        let file_day = match file_mtime_day(path) {
            Some(d) => d,
            None => return,
        };
        if !file_day.starts_with(month) {
            return;
        }

        let Some(models) = parse_claude_transcript(path) else { return };

        for (model, ti, toks, tcr, tcw, cost) in models {
            if ti == 0 && toks == 0 {
                continue;
            }
            let key = (file_day.clone(), model.clone());
            by_day_model
                .entry(key)
                .and_modify(|d| {
                    d.sessions += 1;
                    d.tokens_input += ti;
                    d.tokens_output += toks;
                    d.tokens_cache_read += tcr;
                    d.tokens_cache_write += tcw;
                    d.cost_usd += cost;
                })
                .or_insert(DailyUsage {
                    day: file_day.clone(),
                    cli_id: CliId::Claude.as_str().into(),
                    model_name: model.clone(),
                    sessions: 1,
                    tokens_input: ti,
                    tokens_output: toks,
                    tokens_cache_read: tcr,
                    tokens_cache_write: tcw,
                    cost_usd: cost,
                });
        }
    });

    by_day_model.into_values().collect()
}

/// Parse a Claude Code `.jsonl` transcript, summing token usage per model.
/// Returns a list of `(model, input, output, cache_read, cache_write, cost_usd)`.
fn parse_claude_transcript(
    path: &Path,
) -> Option<ModelUsage> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut by_model: BTreeMap<String, (u64, u64, u64, u64)> = BTreeMap::new();
    let mut last_model = String::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }

        let model = v
            .get("message")
            .and_then(|m| m.get("model"))
            .and_then(|m| m.as_str())
            .unwrap_or("");
        if !model.is_empty() {
            last_model = model.to_string();
        }
        if last_model.is_empty() {
            continue;
        }

        if let Some(usage) = v.get("message").and_then(|m| m.get("usage")) {
            let entry = by_model.entry(last_model.clone()).or_default();
            entry.0 += usage
                .get("input_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            entry.1 += usage
                .get("output_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            entry.2 += usage
                .get("cache_read_input_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            entry.3 += usage
                .get("cache_creation_input_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
        }
    }

    let mut result = Vec::new();
    for (model, (ti, toks, tcr, tcw)) in by_model {
        let cost = pricing::cost_usd(&model, ti, toks, tcr, tcw);
        result.push((model, ti, toks, tcr, tcw, cost));
    }
    if result.is_empty() { None } else { Some(result) }
}

// ════════════════════════════════════════════════════════════════════
// Codex — JSONL rollouts
// ════════════════════════════════════════════════════════════════════

pub fn codex_usage(month: &str) -> Vec<DailyUsage> {
    let Some(profile) = user_profile() else {
        return Vec::new();
    };
    let sessions_dir = profile.join(".codex").join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }

    let mut by_day_model: BTreeMap<(String, String), DailyUsage> = BTreeMap::new();

    walk_files(&sessions_dir, &mut |path| {
        if !is_jsonl(path) {
            return;
        }
        let file_day = match file_mtime_day(path) {
            Some(d) => d,
            None => return,
        };
        if !file_day.starts_with(month) {
            return;
        }

        let Some(models) = parse_codex_transcript(path) else { return };

        for (model, ti, toks, tcr, tcw, cost) in models {
            if ti == 0 && toks == 0 {
                continue;
            }
            let key = (file_day.clone(), model.clone());
            by_day_model
                .entry(key)
                .and_modify(|d| {
                    d.sessions += 1;
                    d.tokens_input += ti;
                    d.tokens_output += toks;
                    d.tokens_cache_read += tcr;
                    d.tokens_cache_write += tcw;
                    d.cost_usd += cost;
                })
                .or_insert(DailyUsage {
                    day: file_day.clone(),
                    cli_id: CliId::Codex.as_str().into(),
                    model_name: model.clone(),
                    sessions: 1,
                    tokens_input: ti,
                    tokens_output: toks,
                    tokens_cache_read: tcr,
                    tokens_cache_write: tcw,
                    cost_usd: cost,
                });
        }
    });

    by_day_model.into_values().collect()
}

/// Parse a Codex rollout `.jsonl`. The exact usage schema varies by Codex
/// version; we probe common field locations best-effort.
/// Returns per-model breakdown.
fn parse_codex_transcript(
    path: &Path,
) -> Option<ModelUsage> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut by_model: BTreeMap<String, (u64, u64, u64, u64)> = BTreeMap::new();
    let mut last_model = String::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        // model
        if let Some(m) = v
            .get("model")
            .and_then(|m| m.as_str())
            .or_else(|| {
                find_usage_object(&v)
                    .and_then(|u| u.get("model").and_then(|m| m.as_str()))
            })
        {
            if !m.is_empty() {
                last_model = m.to_string();
            }
        }
        if last_model.is_empty() {
            continue;
        }

        // Codex usage may be nested under payload.usage, usage, or item.usage.
        let usage = find_usage_object(&v);
        if let Some(u) = usage {
            let entry = by_model.entry(last_model.clone()).or_default();
            entry.0 += u
                .get("prompt_tokens")
                .or_else(|| u.get("input_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            entry.1 += u
                .get("completion_tokens")
                .or_else(|| u.get("output_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            // OpenAI prompt_tokens_details.cached_tokens
            if let Some(details) = u
                .get("prompt_tokens_details")
                .and_then(|t| t.get("cached_tokens"))
                .and_then(|t| t.as_u64())
            {
                entry.2 += details;
            }
        }
    }

    let mut result = Vec::new();
    for (model, (ti, toks, tcr, tcw)) in by_model {
        let cost = pricing::cost_usd(&model, ti, toks, tcr, tcw);
        result.push((model, ti, toks, tcr, tcw, cost));
    }
    if result.is_empty() { None } else { Some(result) }
}

/// Recursively search a JSON value for the first object that looks like a
/// usage payload (contains token-like keys).
fn find_usage_object(v: &Value) -> Option<&Value> {
    if v.is_object() {
        if v.get("prompt_tokens").is_some()
            || v.get("input_tokens").is_some()
            || v.get("completion_tokens").is_some()
            || v.get("output_tokens").is_some()
        {
            return Some(v);
        }
        if let Some(u) = v.get("usage") {
            if u.is_object() {
                return Some(u);
            }
        }
        if let Some(p) = v.get("payload") {
            if let Some(u) = p.get("usage") {
                return Some(u);
            }
            return find_usage_object(p);
        }
    }
    None
}

// ════════════════════════════════════════════════════════════════════
// Shared helpers
// ════════════════════════════════════════════════════════════════════

fn user_profile() -> Option<PathBuf> {
    std::env::var("USERPROFILE").ok().map(PathBuf::from)
}

fn is_jsonl(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "jsonl")
        .unwrap_or(false)
}

fn file_mtime_day(path: &Path) -> Option<String> {
    let meta = path.metadata().ok()?;
    let modified = meta.modified().ok()?;
    let ms = modified
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some(day_from_epoch_ms(ms))
}

/// Recursively visit files only (mirrors `scan.rs::walk_files`).
fn walk_files(root: &Path, f: &mut dyn FnMut(&Path)) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for ent in entries.flatten() {
        let p = ent.path();
        match ent.file_type() {
            Ok(ft) if ft.is_dir() => walk_files(&p, f),
            Ok(ft) if ft.is_file() => f(&p),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_month_format() {
        let m = current_month();
        assert_eq!(m.len(), 7);
        assert_eq!(m.chars().nth(4), Some('-'));
    }

    #[test]
    fn month_from_known_epoch() {
        // 2026-01-15 00:00:00 UTC = 1768435200 epoch seconds = 1768435200000 ms
        let m = month_from_epoch_ms(1_768_435_200_000);
        assert_eq!(m, "2026-01");
    }

    #[test]
    fn day_from_known_epoch() {
        // same epoch as month_from_known_epoch
        let d = day_from_epoch_ms(1_768_435_200_000);
        assert_eq!(d, "2026-01-15");
    }

    #[test]
    fn month_from_january() {
        // 2026-01-15 00:00:00 UTC = 1768435200 epoch seconds = 1768435200000 ms
        let m = month_from_epoch_ms(1_768_435_200_000);
        assert_eq!(m, "2026-01");
    }

    #[test]
    fn month_format_is_yyyy_mm() {
        assert_eq!(current_month().len(), 7);
    }

    #[test]
    fn monthly_usage_empty_when_no_data() {
        let m = monthly_usage("1900-01");
        assert_eq!(m.month, "1900-01");
        assert!(m.daily.is_empty());
        assert!(m.by_cli.is_empty());
        assert_eq!(m.total_tokens(), 0);
        assert_eq!(m.total_cost(), 0.0);
    }

    #[test]
    fn merge_daily_sums_dups() {
        let d = vec![
            DailyUsage {
                day: "2026-07-01".into(),
                cli_id: "claude".into(),
                model_name: "opus-5".into(),
                sessions: 1,
                tokens_input: 100,
                tokens_output: 50,
                tokens_cache_read: 0,
                tokens_cache_write: 0,
                cost_usd: 0.5,
            },
            DailyUsage {
                day: "2026-07-01".into(),
                cli_id: "claude".into(),
                model_name: "opus-5".into(),
                sessions: 1,
                tokens_input: 200,
                tokens_output: 30,
                tokens_cache_read: 0,
                tokens_cache_write: 0,
                cost_usd: 0.3,
            },
        ];
        let merged = merge_daily(d);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].tokens_input, 300);
        assert_eq!(merged[0].sessions, 2);
        assert!((merged[0].cost_usd - 0.8).abs() < 0.001);
    }

    #[test]
    fn build_summary_groups_by_cli() {
        let daily = vec![
            DailyUsage {
                day: "2026-07-01".into(),
                cli_id: "claude".into(),
                model_name: "opus-5".into(),
                sessions: 1,
                tokens_input: 1000,
                tokens_output: 500,
                tokens_cache_read: 0,
                tokens_cache_write: 0,
                cost_usd: 1.0,
            },
            DailyUsage {
                day: "2026-07-02".into(),
                cli_id: "claude".into(),
                model_name: "opus-5".into(),
                sessions: 1,
                tokens_input: 2000,
                tokens_output: 100,
                tokens_cache_read: 0,
                tokens_cache_write: 0,
                cost_usd: 2.0,
            },
            DailyUsage {
                day: "2026-07-01".into(),
                cli_id: "opencode".into(),
                model_name: "gpt-5.6-sol".into(),
                sessions: 1,
                tokens_input: 500,
                tokens_output: 50,
                tokens_cache_read: 0,
                tokens_cache_write: 0,
                cost_usd: 0.5,
            },
        ];
        let summary = build_summary(&daily);
        assert_eq!(summary.len(), 2);
        let claude = summary.iter().find(|s| s.cli_id == "claude").unwrap();
        assert_eq!(claude.sessions, 2);
        assert_eq!(claude.total_tokens_input, 3000);
        assert!((claude.total_cost_usd - 3.0).abs() < 0.001);
    }
}
