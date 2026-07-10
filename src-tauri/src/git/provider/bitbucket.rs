//! Bitbucket provider — REST 2.0 inventory.
//!
//! Bitbucket uses app passwords (Bearer auth). Limit: top-30 repos by
//! recent push activity to stay under Bitbucket's 1000/hr rate limit.

use crate::git::model::{AcctInventory, FailRun, GitAccount, OpenPull, RepoSummary};
use crate::git::provider::auth_err;

const BB_API: &str = "https://api.bitbucket.org/2.0";

pub fn inventory(acct: &GitAccount, token: &str) -> Result<AcctInventory, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let auth = format!("Bearer {token}");

    let repos_url = format!(
        "{BB_API}/repositories?role=member&pagelen=30&sort=-updated_on&fields=values.full_name,values.links.html.href,values.mainbranch.name,values.uuid,values.updated_on"
    );
    let resp = match agent.get(&repos_url).set("Authorization", &auth).call() {
        Ok(r) => r,
        Err(e) => return Ok(auth_err(acct, format!("bitbucket repos: {e}"))),
    };
    let repos_v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(e) => return Ok(auth_err(acct, format!("bitbucket read repos: {e}"))),
    };
    let repos = repos_v
        .pointer("/values")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();

    let mut inv = AcctInventory::empty(acct);
    let mut failed = Vec::new();
    let mut pulls = Vec::new();

    for repo in &repos {
        let full_name = str_or(repo, "/full_name", "").to_string();
        let web_url = link_href(repo, "/links/html");
        let default_branch = str_or(repo, "/mainbranch/name", "main").to_string();
        let uuid = str_or(repo, "/uuid", "").to_string();

        // pipelines (filter only FAILED/CANCELLED)
        let pipe_url = format!(
            "{BB_API}/repositories/{full_name}/pipelines/?pagelen=10&sort=-created_on&fields=values.state.name,values.target.commit.hash,values.created_on,values.links.html.href"
        );
        let (pipe_state, short_sha, finished_iso, pipe_url_out) = fetch_pipes(&agent, &auth, &pipe_url);

        // open PRs
        let pr_url = format!(
            "{BB_API}/repositories/{full_name}/pullrequests?state=OPEN&pagelen=20&fields=values.id,values.title,values.source.branch.name,values.author.display_name,values.links.html.href"
        );
        let open_prs = fetch_items(&agent, &auth, &pr_url);
        let open_pr_count = open_prs.len() as u32;
        for pr in &open_prs {
            let number = pr.pointer("/id").and_then(|n| n.as_u64()).unwrap_or(0);
            let title = str_or(pr, "/title", "").to_string();
            let author_display = str_or(pr, "/author/display_name", "?").to_string();
            let branch = str_or(pr, "/source/branch/name", "").to_string();
            let pr_web_url = link_href(pr, "/links/html");
            pulls.push(OpenPull {
                provider: "bitbucket".into(),
                full_name: full_name.clone(),
                number,
                title,
                author_display,
                is_draft: false,
                branch,
                web_url: pr_web_url,
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }
        if pipe_state == "failed" || pipe_state == "cancelled" {
            failed.push(FailRun {
                provider: "bitbucket".into(),
                full_name: full_name.clone(),
                run_label: "pipeline".into(),
                branch: default_branch.clone(),
                short_sha: short_sha.chars().take(7).collect(),
                ago: crate::git::provider::ago_from_iso(&finished_iso),
                finished_ms: crate::git::provider::ago_from_iso_ms(&finished_iso),
                web_url: pipe_url_out,
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }
        let _ = uuid; // uuid currently unused — placeholder for v2 self-hosted bitbucket support
        inv.repos.push(RepoSummary {
            full_name,
            provider: "bitbucket".into(),
            last_state: pipe_state,
            open_prs: open_pr_count,
            default_branch_sha: short_sha.chars().take(7).collect(),
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

fn fetch_pipes(
    agent: &ureq::Agent,
    auth: &str,
    url: &str,
) -> (String, String, String, String) {
    let resp = match agent.get(url).set("Authorization", auth).call() {
        Ok(r) => r,
        Err(_) => return ("unknown".into(), String::new(), String::new(), String::new()),
    };
    let v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return ("unknown".into(), String::new(), String::new(), String::new()),
    };
    let pipes = v
        .pointer("/values")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    // Pick the most recent COMPLETED (not IN_PROGRESS) pipeline.
    let pick = pipes.iter().find(|p| {
        matches!(
            str_or(p, "/state/name", ""),
            "COMPLETED" | "FAILED" | "SUCCESSFUL" | "STOPPED" | "CANCELLED" | "ERROR" | "EXPIRED"
        )
    });
    let Some(p) = pick else { return ("unknown".into(), String::new(), String::new(), String::new()); };
    let result = p.pointer("/state/result/name").and_then(|n| n.as_str()).unwrap_or("");
    let mapped = match result {
        "SUCCESSFUL" => "success",
        "FAILED" => "failed",
        "ERROR" => "failed",
        "STOPPED" | "CANCELLED" => "cancelled",
        "" => match str_or(p, "/state/name", "") {
            "IN_PROGRESS" | "PENDING" => "running",
            _ => "unknown",
        },
        _ => "unknown",
    };
    let sha = str_or(p, "/target/commit/hash", "").to_string();
    let finished_iso = str_or(p, "/created_on", "").to_string();
    let web_url = link_href(p, "/links/html");
    (mapped.into(), sha, finished_iso, web_url)
}

fn fetch_items(agent: &ureq::Agent, auth: &str, url: &str) -> Vec<serde_json::Value> {
    let resp = match agent.get(url).set("Authorization", auth).call() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v.pointer("/values")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default()
}

fn str_or<'a>(v: &'a serde_json::Value, ptr: &str, default: &'a str) -> &'a str {
    v.pointer(ptr).and_then(|n| n.as_str()).unwrap_or(default)
}

fn link_href(v: &serde_json::Value, ptr: &str) -> String {
    v.pointer(ptr)
        .and_then(|n| n.pointer("/href"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
