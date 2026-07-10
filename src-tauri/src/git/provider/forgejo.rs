//! Forgejo (Gitea) provider — REST v1 inventory.
//!
//! Self-hosted only (no cloud). Uses `host_url/api/v1/...`.
//! Endpoints:
//! - `GET /user/repos` — list repos for authenticated user
//! - `GET /repos/{owner}/{repo}/pulls?state=open` — open PRs
//! - `GET /repos/{owner}/{repo}/commits/{ref}/status` — combined CI status

use crate::git::model::{AcctInventory, FailRun, GitAccount, OpenPull, RepoSummary};
use crate::git::provider::auth_err;

pub fn inventory(acct: &GitAccount, token: &str) -> Result<AcctInventory, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let auth = format!("token {token}");
    let base = format!(
        "{}/api/v1",
        acct.host_url.trim_end_matches('/')
    );

    // 1) list repos for the user
    let repos_url = format!("{base}/user/repos?limit=50");
    let resp = match agent
        .get(&repos_url)
        .set("Authorization", &auth)
        .set("Accept", "application/json")
        .call()
    {
        Ok(r) => r,
        Err(e) => return Ok(auth_err(acct, format!("forgejo repos: {e}"))),
    };
    let repos_v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(e) => return Ok(auth_err(acct, format!("forgejo read repos: {e}"))),
    };
    let repos = repos_v.as_array().cloned().unwrap_or_default();

    let mut inv = AcctInventory::empty(acct);
    let mut failed = Vec::new();
    let mut pulls = Vec::new();

    for repo in &repos {
        let owner = str_or(repo, "/owner/login", "");
        let name = str_or(repo, "/name", "");
        let full_name = str_or(repo, "/full_name", &format!("{owner}/{name}")).to_string();
        let web_url = str_or(repo, "/html_url", "").to_string();
        let default_branch = str_or(repo, "/default_branch", "main").to_string();

        // 2) combined CI status on default branch
        let status_url = format!(
            "{base}/repos/{owner}/{name}/commits/{default_branch}/status"
        );
        let (last_state, last_sha, finished_ms, ago) = fetch_status(&agent, &auth, &status_url);

        // 3) open PRs
        let pr_url = format!(
            "{base}/repos/{owner}/{name}/pulls?state=open&limit=20"
        );
        let open_prs = fetch_items(&agent, &auth, &pr_url);
        let open_pr_count = open_prs.len() as u32;

        for pr in &open_prs {
            let number = pr.pointer("/number").and_then(|n| n.as_u64()).unwrap_or(0);
            let title = str_or(pr, "/title", "").to_string();
            let author_display = str_or(pr, "/user/login", "?").to_string();
            let is_draft = pr.pointer("/draft").and_then(|n| n.as_bool()).unwrap_or(false);
            let branch = str_or(pr, "/head/ref", "").to_string();
            let pr_web_url = str_or(pr, "/html_url", "").to_string();
            pulls.push(OpenPull {
                provider: "forgejo".into(),
                full_name: full_name.clone(),
                number,
                title,
                author_display,
                is_draft,
                branch,
                web_url: pr_web_url,
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }

        if last_state == "failure" || last_state == "error" {
            failed.push(FailRun {
                provider: "forgejo".into(),
                full_name: full_name.clone(),
                run_label: "latest CI".into(),
                branch: default_branch.clone(),
                short_sha: last_sha.chars().take(7).collect(),
                ago,
                finished_ms,
                web_url: format!("{web_url}/actions"),
                error: String::new(),
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }

        inv.repos.push(RepoSummary {
            full_name,
            provider: "forgejo".into(),
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

fn fetch_status(agent: &ureq::Agent, auth: &str, url: &str) -> (String, String, i64, String) {
    let resp = match agent.get(url).set("Authorization", auth).set("Accept", "application/json").call() {
        Ok(r) => r,
        Err(_) => return ("unknown".into(), String::new(), 0, String::new()),
    };
    let v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return ("unknown".into(), String::new(), 0, String::new()),
    };
    let state = str_or(&v, "/state", "unknown");
    let mapped = match state {
        "success" => "success",
        "failure" | "error" => "failed",
        "pending" => "running",
        "warning" => "running",
        _ => "unknown",
    };
    let sha = str_or(&v, "/sha", "").to_string();
    let finished = v
        .pointer("/statuses")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.pointer("/created_at"))
        .and_then(|n| n.as_str())
        .map(crate::git::provider::ago_from_iso_ms)
        .unwrap_or(0);
    let ago = v
        .pointer("/statuses")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.pointer("/created_at"))
        .and_then(|n| n.as_str())
        .map(crate::git::provider::ago_from_iso)
        .unwrap_or_default();
    (mapped.into(), sha, finished, ago)
}

fn fetch_items(agent: &ureq::Agent, auth: &str, url: &str) -> Vec<serde_json::Value> {
    let resp = match agent.get(url).set("Authorization", auth).set("Accept", "application/json").call() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v.as_array().cloned().unwrap_or_default()
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
