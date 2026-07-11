//! GitHub provider — one GraphQL call to `/graphql` covering repos +
//! latest CI state + open PRs. Minimum API surface.
//!
//! Token: classic PAT or fine-grained PAT with `metadata:read` +
//! `pull_requests:read` + `actions:read`. CI state is derived from BOTH
//! the legacy Commit Status API (`status.state`) AND GitHub Actions
//! (`checkSuites`) — reading only `status` produces a false `"unknown"`
//! for the many repos that report results exclusively via Actions.

use crate::git::model::{AcctInventory, FailRun, GitAccount, OpenPull, RepoSummary};

const GITHUB_GRAPHQL: &str = "https://api.github.com/graphql";

const QUERY: &str = r#"
query ZenithInventory {
  viewer {
    login
    repositories(first: 50, ownerAffiliations: [OWNER], orderBy: {field: PUSHED_AT, direction: DESC}) {
      nodes {
        nameWithOwner
        url
        defaultBranchRef {
          name
          target {
            ... on Commit {
              oid
              status {
                state
                contexts { state targetUrl }
              }
              checkSuites(first: 5) {
                nodes {
                  status
                  conclusion
                  url
                  commit { committedDate }
                  checkRuns(first: 5) {
                    nodes {
                      name
                      conclusion
                      status
                      title
                      summary
                      text
                    }
                  }
                }
              }
            }
          }
        }
        pullRequests(states: [OPEN], first: 20, orderBy: {field: CREATED_AT, direction: DESC}) {
          nodes {
            number
            title
            isDraft
            headRefName
            author { login }
            url
          }
        }
      }
    }
  }
}"#;

pub fn inventory(acct: &GitAccount, token: &str) -> Result<AcctInventory, String> {
    let query = QUERY.replace("__default__", "main");
    let body = serde_json::json!({ "query": query });
    let resp = ureq::post(GITHUB_GRAPHQL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", &format!("zenith/{}", acct.username))
        .send_string(&body.to_string())
        .map_err(|e| format!("github: {e}"))?;
    let v: serde_json::Value = resp.into_json().map_err(|e| format!("github read: {e}"))?;

    // Check for GraphQL errors before extracting data — a token with
    // insufficient scopes (or a revoked token) produces a 200 with an
    // `errors` array. Without this check, `/data/viewer/repositories/nodes`
    // returns None, repos is empty, last_error stays "", and the account
    // is completely invisible in the UI (no repos, no error).
    if let Some(errs) = v.pointer("/errors").and_then(|e| e.as_array()) {
        if !errs.is_empty() {
            let msgs: Vec<String> = errs.iter().map(|e| {
                e.pointer("/message").and_then(|m| m.as_str()).unwrap_or("unknown GraphQL error").to_string()
            }).collect();
            return Err(msgs.join("; "));
        }
    }

    let mut inv = AcctInventory::empty(acct);
    let mut failed = Vec::new();
    let mut pulls = Vec::new();
    let mut open_pr_total = 0u32;
    let repos = v
        .pointer("/data/viewer/repositories/nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    if v.pointer("/data/viewer").is_none() && v.pointer("/data").is_some() {
        return Err("github: token lacks viewer access — check token scopes".into());
    }
    for repo in repos {
        let full_name = str_or(&repo, "/nameWithOwner", "").to_string();
        let web_url = str_or(&repo, "/url", "").to_string();
        let default_branch = str_or(&repo, "/defaultBranchRef/name", "main").to_string();
        let pr_arr = repo
            .pointer("/pullRequests/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        let open_prs = pr_arr.len() as u32;
        open_pr_total = open_pr_total.saturating_add(open_prs);
        for pr in pr_arr {
            let number = pr.pointer("/number").and_then(|n| n.as_u64()).unwrap_or(0);
            let title = str_or(&pr, "/title", "").to_string();
            let author_display = str_or(&pr, "/author/login", "?").to_string();
            let is_draft = pr.pointer("/isDraft").and_then(|n| n.as_bool()).unwrap_or(false);
            let branch = str_or(&pr, "/headRefName", "").to_string();
            let web_url = str_or(&pr, "/url", "").to_string();
            pulls.push(OpenPull {
                provider: "github".into(),
                full_name: full_name.clone(),
                number,
                title,
                author_display,
                is_draft,
                branch,
                web_url,
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }
        let ci = commit_ci(&repo);
        inv.repos.push(RepoSummary {
            full_name: full_name.clone(),
            provider: "github".into(),
            last_state: ci.state.clone(),
            open_prs,
            default_branch_sha: ci.short_sha.clone(),
            default_branch: default_branch.clone(),
            web_url,
        });
        if ci.state == "failed" || ci.state == "cancelled" {
            // For GitHub we surface ALL "failed"/"cancelled" refs as FailRun
            // entries — the GraphQL response has no per-run table, so the chip
            // count is "number of repos whose latest default-branch CI
            // failed / cancelled" rather than total run count. This is per the
            // user's "actions that fail" intent: one chip per broken repo.
            failed.push(FailRun {
                provider: "github".into(),
                full_name: full_name.clone(),
                run_label: if ci.run_label.is_empty() {
                    "latest default-branch CI".into()
                } else {
                    ci.run_label.clone()
                },
                branch: default_branch.clone(),
                short_sha: ci.short_sha,
                ago: String::new(),
                finished_ms: ci.finished_ms,
                web_url: ci.web_url.clone(),
                error: ci.error.clone(),
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }
    }
    inv.failed_runs = failed;
    inv.open_pulls = pulls;
    inv.last_sync_ms = now_ms();
    inv.last_error = String::new();
    if open_pr_total == 0 && inv.repos.is_empty() {
        // Auth success but zero repos — leave empty.
    }
    Ok(inv)
}

fn str_or<'a>(v: &'a serde_json::Value, ptr: &str, default: &'a str) -> &'a str {
    v.pointer(ptr).and_then(|n| n.as_str()).unwrap_or(default)
}

/// Combined CI state for a repo's default-branch head commit.
///
/// Reads BOTH the legacy Commit Status API (`status.state`) and GitHub
/// Actions (`checkSuites`). Reading only `status` yields a false
/// `"unknown"` for the many repos that report exclusively via Actions, so
/// the result here is the union of the two sources.
struct CommitCi {
    state: String,
    short_sha: String,
    /// Label for a representative failed/cancelled run (e.g. "GitHub Actions").
    run_label: String,
    /// Web URL of the failed/cancelled run (empty when none).
    web_url: String,
    /// Unix millis the failed/cancelled run finished, 0 when unknown.
    finished_ms: i64,
    /// Short failure summary (check-run output), empty when unavailable.
    error: String,
}

fn commit_ci(repo: &serde_json::Value) -> CommitCi {
    let target = repo.pointer("/defaultBranchRef/target");
    let oid = target
        .and_then(|t| t.pointer("/oid"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    let short = oid.chars().take(7).collect::<String>();

    let status_state = target
        .and_then(|t| t.pointer("/status/state"))
        .and_then(|n| n.as_str())
        .unwrap_or("");

    let suites = target
        .and_then(|t| t.pointer("/checkSuites/nodes"))
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();

    let mut has_failed = false;
    let mut has_running = false;
    let mut has_success = false;
    let mut fail_url = String::new();
    let mut fail_finished = 0i64;
    let mut fail_label = String::new();

    // Legacy Commit Status API.
    match status_state {
        "SUCCESS" => has_success = true,
        "FAILURE" | "ERROR" => has_failed = true,
        "PENDING" | "EXPECTED" => has_running = true,
        _ => {}
    }

    // GitHub Actions check suites.
    for s in &suites {
        let status = str_or(s, "/status", "");
        let conclusion = str_or(s, "/conclusion", "");
        let url = str_or(s, "/url", "");
        let finished = super::ago_from_iso_ms(str_or(s, "/commit/committedDate", ""));
        match conclusion {
            "FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" => {
                has_failed = true;
                if fail_url.is_empty() {
                    fail_url = url.to_string();
                    fail_finished = finished;
                    fail_label = "GitHub Actions".into();
                }
            }
            "CANCELLED" => {
                has_failed = true;
                if fail_url.is_empty() {
                    fail_url = url.to_string();
                    fail_finished = finished;
                    fail_label = "GitHub Actions (cancelled)".into();
                }
            }
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => has_success = true,
            "" => match status {
                // Only IN_PROGRESS is genuinely running. QUEUED / REQUESTED /
                // WAITING / PENDING are queued-or-pending states that are not
                // actively executing — counting them as "running" produces
                // permanent false positives for suites stuck in a queued state
                // (e.g. requested but never picked up by a runner).
                "IN_PROGRESS" => has_running = true,
                "COMPLETED" => has_success = true,
                _ => {}
            },
            _ => {}
        }
    }

    let state = if has_failed {
        "failed".to_string()
    } else if has_running {
        "running".to_string()
    } else if has_success {
        "success".to_string()
    } else {
        "unknown".to_string()
    };

    // Capture a short failure summary from the first failed check-run's output.
    let error = capture_error(target);

    CommitCi {
        state,
        short_sha: short,
        run_label: fail_label,
        web_url: fail_url,
        finished_ms: fail_finished,
        error,
    }
}

/// Pull a short, human-readable failure summary from the first failed
/// check-run's `output` (title/summary/text). Truncated so the AI prompt
/// stays bounded.
fn capture_error(target: Option<&serde_json::Value>) -> String {
    let Some(target) = target else { return String::new() };
    let suites = target
        .pointer("/checkSuites/nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    const MAX: usize = 1200;
    for suite in &suites {
        let runs = suite
            .pointer("/checkRuns/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        for run in &runs {
            let conclusion = str_or(run, "/conclusion", "");
            let is_failed = matches!(
                conclusion,
                "FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "CANCELLED"
            );
            if !is_failed {
                continue;
            }
            let name = str_or(run, "/name", "check");
            let mut parts: Vec<String> = Vec::new();
            if let Some(t) = run.pointer("/title").and_then(|n| n.as_str()) {
                if !t.is_empty() {
                    parts.push(t.to_string());
                }
            }
            if let Some(s) = run.pointer("/summary").and_then(|n| n.as_str()) {
                if !s.is_empty() {
                    parts.push(s.to_string());
                }
            }
            if let Some(t) = run.pointer("/text").and_then(|n| n.as_str()) {
                if !t.is_empty() {
                    parts.push(t.to_string());
                }
            }
            if parts.is_empty() {
                continue;
            }
            let mut text = format!("{name}: {}", parts.join(" — "));
            if text.chars().count() > MAX {
                text = text.chars().take(MAX).collect::<String>();
                text.push_str("…");
            }
            return text;
        }
    }
    String::new()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
