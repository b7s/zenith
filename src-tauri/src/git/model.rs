//! DTOs for the git domain. Mirrored in `src/shared/types.ts` — keep the two
//! in sync (comment links the files).

use serde::{Deserialize, Serialize};

/// One configured account. Persisted under
/// `widgets.config.git.accounts[]` in `config.json`. `token_blob` is a
/// base64-encoded DPAPI-protected blob — never plaintext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAccount {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub username: String,
    /// Optional self-hosted instance URL (e.g. `https://gitlab.example.com`
    /// or `https://bitbucket.example.com`). Empty = use cloud default
    /// (`api.bitbucket.org` / `gitlab.com` / `github.com`).
    #[serde(default)]
    pub host_url: String,
    /// base64(DPAPI-protected token bytes). Never plaintext on disk.
    /// `#[serde(alias = "token")]` migrates old configs that used the
    /// plaintext `token` key before DPAPI protection was implemented.
    #[serde(default, alias = "token")]
    pub token_blob: String,
    #[serde(default = "default_poll_mins")]
    pub poll_mins: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_poll_mins() -> u64 { 5 }
fn default_enabled() -> bool { true }

/// Top-level widget config persisted under `widgets.config.git`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitWidgetConfig {
    #[serde(default)]
    pub accounts: Vec<GitAccount>,
    /// `None` = "All".
    #[serde(default)]
    pub selected_account_id: Option<String>,
    #[serde(default = "default_global_poll")]
    pub poll_interval_mins: u64,
    /// Only failed CI runs finished within this many days are counted
    /// (drives the dot, title, dashboard, tabs, charts). 0 = no limit.
    #[serde(default = "default_window_days")]
    pub failures_window_days: u64,
}

fn default_global_poll() -> u64 { 5 }
fn default_window_days() -> u64 { 14 }

impl Default for GitWidgetConfig {
    fn default() -> Self {
        Self {
            accounts: Vec::new(),
            selected_account_id: None,
            poll_interval_mins: default_global_poll(),
            failures_window_days: default_window_days(),
        }
    }
}

/// Combined snapshot for a single account — last successful inventory
/// plus its metadata. Emitted in aggregate by `poll::GitState`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcctInventory {
    pub account_id: String,
    pub account_label: String,
    pub provider: String,
    pub username: String,
    pub repos: Vec<RepoSummary>,
    pub failed_runs: Vec<FailRun>,
    pub open_pulls: Vec<OpenPull>,
    /// Unix millis of the last successful sync.
    #[serde(default)]
    pub last_sync_ms: i64,
    /// Last error string (human), or empty when none.
    #[serde(default)]
    pub last_error: String,
}

impl AcctInventory {
    pub fn empty(acct: &GitAccount) -> Self {
        Self {
            account_id: acct.id.clone(),
            account_label: acct.label.clone(),
            provider: acct.provider.clone(),
            username: acct.username.clone(),
            repos: vec![],
            failed_runs: vec![],
            open_pulls: vec![],
            last_sync_ms: 0,
            last_error: String::new(),
        }
    }
}

/// A repo's identity + its most-recent CI state + open-PR count.
/// Drives the Overview tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSummary {
    pub full_name: String,
    pub provider: String,
    /// `"failed"` | `"success"` | `"running"` | `"cancelled"` | `"unknown"`.
    pub last_state: String,
    /// Number of open PRs/MRs.
    pub open_prs: u32,
    /// Short SHA of latest commit on default branch (last 7 chars).
    #[serde(default)]
    pub default_branch_sha: String,
    /// Default branch name.
    #[serde(default)]
    pub default_branch: String,
    /// Web URL of the repo.
    #[serde(default)]
    pub web_url: String,
}

/// A failed CI run. Drives the Failed CI tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailRun {
    pub provider: String,
    pub full_name: String,
    pub run_label: String,
    pub branch: String,
    pub short_sha: String,
    /// "12m ago" style relative string filled by the provider.
    #[serde(default)]
    pub ago: String,
    /// Unix millis when the run finished, 0 if unknown.
    #[serde(default)]
    pub finished_ms: i64,
    /// Web URL to the failed run page.
    #[serde(default)]
    pub web_url: String,
    /// Short failure summary from the CI provider (e.g. a failed check-run's
    /// output), or empty when unavailable. Surfaced to AI assistants.
    #[serde(default)]
    pub error: String,
    /// Owning account id — used by the account filter.
    pub account_id: String,
    /// Owning account label — display.
    pub account_label: String,
}

/// An open PR/MR awaiting review. Drives the Open PRs tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPull {
    pub provider: String,
    pub full_name: String,
    pub number: u64,
    pub title: String,
    pub author_display: String,
    pub is_draft: bool,
    pub branch: String,
    /// Web URL of the PR/MR.
    #[serde(default)]
    pub web_url: String,
    /// Owning account metadata.
    pub account_id: String,
    pub account_label: String,
}

/// Aggregated state across all enabled accounts. Cached by the poll
/// thread, surfaced to the frontend via `get_git_state` and the
/// `zenith:git-changed` event payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitState {
    #[serde(default)]
    pub inventories: Vec<AcctInventory>,
    #[serde(default)]
    pub total_failed: u32,
    #[serde(default)]
    pub total_open_prs: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum ProviderKind {
        Github,
        Gitlab,
        Bitbucket,
    }

    impl ProviderKind {
        fn as_str(&self) -> &'static str {
            match self {
                Self::Github => "github",
                Self::Gitlab => "gitlab",
                Self::Bitbucket => "bitbucket",
            }
        }

        fn parse(s: &str) -> Option<Self> {
            match s.to_ascii_lowercase().as_str() {
                "github" => Some(Self::Github),
                "gitlab" => Some(Self::Gitlab),
                "bitbucket" => Some(Self::Bitbucket),
                _ => None,
            }
        }
    }

    #[test]
    fn provider_parse_roundtrip() {
        for s in ["github", "gitlab", "bitbucket"] {
            let p = ProviderKind::parse(s).unwrap();
            assert_eq!(p.as_str(), s);
        }
        assert!(ProviderKind::parse("nope").is_none());
    }

    #[test]
    fn git_widget_config_defaults() {
        let cfg = GitWidgetConfig::default();
        assert!(cfg.accounts.is_empty());
        assert_eq!(cfg.poll_interval_mins, 5);
        assert_eq!(cfg.failures_window_days, 14);
        assert!(cfg.selected_account_id.is_none());
    }

    #[test]
    fn account_default_poll_mins_through_deserialize() {
        let raw = r#"{ "id": "x", "label": "L", "provider": "github", "username": "u" }"#;
        let acct: GitAccount = serde_json::from_str(raw).unwrap();
        assert_eq!(acct.poll_mins, 5);
        assert!(acct.enabled);
        assert_eq!(acct.token_blob, "");
    }
}
