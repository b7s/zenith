//! GitLab provider — REST v4 inventory.
//!
//! Two calls per poll: list repos (`GET /projects`), then per-repo latest
//! pipeline (`GET /projects/:id/pipelines?per_page=1`) + open MRs
//! (`GET /merge_requests?state=opened&scope=all`). To stay under
//! GitLab's per-minute budget we cap at top-50 repos by last activity.
//! Self-hosted GitLab supported via `acct.host_url`.

use crate::git::model::{AcctInventory, FailRun, GitAccount, OpenPull, RepoSummary};
use crate::git::provider::auth_err;

const GITLAB_CLOUD_API: &str = "https://gitlab.com/api/v4";

fn api_base(acct: &GitAccount) -> String {
    let h = acct.host_url.trim_end_matches('/');
    if h.is_empty() {
        GITLAB_CLOUD_API.into()
    } else {
        format!("{h}/api/v4")
    }
}

pub fn inventory(acct: &GitAccount, token: &str) -> Result<AcctInventory, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let auth_header = format!("Bearer {token}");
    let auth = |req: ureq::Request| req.set("PRIVATE-TOKEN", token);
    let api = api_base(acct);

    let repos_url = format!(
        "{api}/projects?membership=true&per_page=50&order_by=last_activity_at"
    );
    let resp = match auth(
        agent.get(&repos_url).set("Authorization", &auth_header)
    ).call() {
        Ok(r) => r,
        Err(e) => return Ok(auth_err(acct, format!("gitlab repos: {e}"))),
    };
    let repos_v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(e) => return Ok(auth_err(acct, format!("gitlab read repos: {e}"))),
    };
    let repos = repos_v.as_array().cloned().unwrap_or_default();

    let mut inv = AcctInventory::empty(acct);
    let mut failed = Vec::new();
    let mut pulls = Vec::new();

    for repo in &repos {
        let id = repo.pointer("/id").and_then(|n| n.as_i64()).unwrap_or(0);
        let full_name = str_or(repo, "/path_with_namespace", "").to_string();
        let web_url = str_or(repo, "/web_url", "").to_string();
        let default_branch = str_or(repo, "/default_branch", "main").to_string();
        // 2) latest pipeline on default branch
        let pipeline_url = format!(
            "{api}/projects/{id}/pipelines?per_page=5&ref={default_branch}"
        );
        let pipe_state = fetch_pipes(&agent, &auth_header, &pipeline_url);
        // 3) open MRs for the project
        let mr_url = format!(
            "{api}/projects/{id}/merge_requests?state=opened&per_page=20&scope=all"
        );
        let open_mrs = fetch_mrs(&agent, &auth_header, &mr_url);
        let (last_state, last_sha, last_finished, ago) = summarize_pipes(&pipe_state);
        let open_pr_count = open_mrs.len() as u32;
        for pr in &open_mrs {
            let number = pr.pointer("/iid").and_then(|n| n.as_u64()).unwrap_or(0);
            let title = str_or(pr, "/title", "").to_string();
            let author_display = str_or(pr, "/author/username", "?").to_string();
            let is_draft =
                pr.pointer("/work_in_progress").and_then(|n| n.as_bool()).unwrap_or(false);
            let branch = str_or(pr, "/source_branch", "").to_string();
            let web_url = str_or(pr, "/web_url", "").to_string();
            pulls.push(OpenPull {
                provider: "gitlab".into(),
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
        if last_state == "failed" || last_state == "cancelled" {
            failed.push(FailRun {
                provider: "gitlab".into(),
                full_name: full_name.clone(),
                run_label: "latest pipeline".into(),
                branch: default_branch.clone(),
                short_sha: last_sha.chars().take(7).collect(),
                ago,
                finished_ms: last_finished,
                web_url: format!("{web_url}/-/pipelines"),
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }
        inv.repos.push(RepoSummary {
            full_name,
            provider: "gitlab".into(),
            last_state,
            open_prs: open_pr_count,
            default_branch_sha: last_sha.chars().take(7).collect(),
            default_branch,
            web_url,
        });
    }

    inv.failed_runs = failed;
    inv.open_pulls = pulls;
    inv.last_sync_ms = now_ms();
    inv.last_error = String::new();
    Ok(inv)
}

fn fetch_pipes(agent: &ureq::Agent, auth: &str, url: &str) -> Vec<serde_json::Value> {
    let resp = match agent.get(url).set("Authorization", auth).call() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v.as_array().cloned().unwrap_or_default()
}

fn fetch_mrs(agent: &ureq::Agent, auth: &str, url: &str) -> Vec<serde_json::Value> {
    fetch_pipes(agent, auth, url)
}

fn summarize_pipes(pipes: &[serde_json::Value]) -> (String, String, i64, String) {
    // Pick the most-recent completed pipeline (first entry is most recent).
    let pick = pipes.first();
    let Some(p) = pick else { return ("unknown".into(), String::new(), 0, String::new()); };
    let status = str_or(p, "/status", "unknown");
    let mapped = match status {
        "success" => "success",
        "failed" => "failed",
        "running" | "pending" => "running",
        "canceled" => "cancelled",
        _ => "unknown",
    };
    let sha = str_or(p, "/sha", "").to_string();
    let finished = p
        .pointer("/updated_at")
        .and_then(|n| n.as_str())
        .map(crate::git::provider::ago_from_iso_ms)
        .unwrap_or(0);
    let ago = p
        .pointer("/updated_at")
        .and_then(|n| n.as_str())
        .map(crate::git::provider::ago_from_iso)
        .unwrap_or_default();
    (mapped.into(), sha, finished, ago)
}

fn str_or<'a>(v: &'a serde_json::Value, ptr: &str, default: &'a str) -> &'a str {
    v.pointer(ptr).and_then(|n| n.as_str()).unwrap_or(default)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
