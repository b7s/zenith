//! Bitbucket Server (Data Center) provider — REST 1.0 inventory.
//!
//! Used when `acct.host_url` is non-empty. Endpoints:
//! - `/rest/api/1.0/projects` — list projects
//! - `/rest/api/1.0/projects/{key}/repos` — list repos per project
//! - `/rest/api/1.0/dashboard/pull-requests` — all open PRs
//!
//! Bitbucket Server has no universal CI pipeline API via REST.
//! Repo `last_state` is always "unknown" (no pipeline data).

use crate::git::model::{AcctInventory, GitAccount, OpenPull, RepoSummary};
use crate::git::provider::auth_err;

pub fn inventory(acct: &GitAccount, token: &str) -> Result<AcctInventory, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let auth = format!("Bearer {token}");
    let base = format!(
        "{}/rest/api/1.0",
        acct.host_url.trim_end_matches('/')
    );

    // 1) list projects
    let proj_url = format!("{base}/projects?limit=50");
    let proj_resp = match agent
        .get(&proj_url)
        .set("Authorization", &auth)
        .set("Accept", "application/json")
        .call()
    {
        Ok(r) => r,
        Err(e) => return Ok(auth_err(acct, format!("bitbucket-server projects: {e}"))),
    };
    let proj_v: serde_json::Value = match proj_resp.into_json() {
        Ok(v) => v,
        Err(e) => return Ok(auth_err(acct, format!("bitbucket-server read projects: {e}"))),
    };
    let projects = proj_v
        .pointer("/values")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();

    if projects.is_empty() {
        let mut inv = AcctInventory::empty(acct);
        inv.last_error = "no projects found".into();
        return Ok(inv);
    }

    // 2) collect repos from all projects
    let mut all_repos: Vec<(String, String, String)> = Vec::new(); // (full_name, web_url, project_key/repo_slug)
    let mut repo_identities: Vec<(String, String)> = Vec::new(); // (project_key, repo_slug) for PR matching

    for proj in &projects {
        let key = str_or(proj, "/key", "");
        let proj_name = str_or(proj, "/name", "");
        let repos_url = format!("{base}/projects/{key}/repos?limit=50");
        let repos_resp = match agent
            .get(&repos_url)
            .set("Authorization", &auth)
            .set("Accept", "application/json")
            .call()
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        let repos_v: serde_json::Value = match repos_resp.into_json() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let repos = repos_v
            .pointer("/values")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        for repo in &repos {
            let slug = str_or(repo, "/slug", "");
            let name = str_or(repo, "/name", "");
            let full_name = format!("{}/{}", proj_name, name);
            let web_url = format!(
                "{}/projects/{}/repos/{}/browse",
                acct.host_url.trim_end_matches('/'),
                key,
                slug
            );
            all_repos.push((full_name, web_url, key.to_string() + "/" + slug));
            repo_identities.push((key.to_string(), slug.to_string()));
        }
    }

    // 3) all open PRs from dashboard
    let pr_url = format!("{base}/dashboard/pull-requests?state=OPEN&limit=50");
    let pr_resp = match agent
        .get(&pr_url)
        .set("Authorization", &auth)
        .set("Accept", "application/json")
        .call()
    {
        Ok(r) => r,
        Err(e) => return Ok(auth_err(acct, format!("bitbucket-server PRs: {e}"))),
    };
    let pr_v: serde_json::Value = match pr_resp.into_json() {
        Ok(v) => v,
        Err(e) => return Ok(auth_err(acct, format!("bitbucket-server read PRs: {e}"))),
    };
    let prs = pr_v
        .pointer("/values")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();

    // Build a lookup: (project_key, repo_slug) → full_name for matching PRs to repos
    let mut repo_lookup: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    for (full_name, _, identity) in &all_repos {
        if let Some((pk, rs)) = identity.split_once('/') {
            repo_lookup
                .entry((pk.to_string(), rs.to_string()))
                .or_insert_with(|| full_name.clone());
        }
    }

    let mut inv = AcctInventory::empty(acct);
    let mut pulls = Vec::new();

    // Index PRs by repo
    struct PrInfo {
        number: u64,
        title: String,
        author: String,
        branch: String,
        web_url: String,
    }
    let mut prs_by_repo: std::collections::HashMap<String, Vec<PrInfo>> =
        std::collections::HashMap::new();

    for pr in &prs {
        let number = pr.pointer("/id").and_then(|n| n.as_u64()).unwrap_or(0);
        let title = str_or(pr, "/title", "").to_string();
        let author = pr
            .pointer("/author/user/displayName")
            .or_else(|| pr.pointer("/author/user/name"))
            .and_then(|n| n.as_str())
            .unwrap_or("?")
            .to_string();
        let branch = str_or(pr, "/fromRef/displayId", "").to_string();
        // Prefer `links.html[0].href` (browser URL), fall back to `links.self[0].href` (REST API)
        let pr_links_html = pr.pointer("/links/html").and_then(|a| a.as_array());
        let pr_web_url = pr_links_html
            .and_then(|arr| arr.first())
            .and_then(|n| n.pointer("/href"))
            .or_else(|| {
                pr.pointer("/links/self")
                    .and_then(|a| a.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|n| n.pointer("/href"))
            })
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();

        let repo_slug = str_or(pr, "/fromRef/repository/slug", "").to_string();
        let proj_key = pr
            .pointer("/fromRef/repository/project/key")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();

        let full_name = repo_lookup
            .get(&(proj_key.clone(), repo_slug.clone()))
            .cloned()
            .unwrap_or_else(|| format!("{}/{}", proj_key, repo_slug));

        prs_by_repo
            .entry(full_name.clone())
            .or_default()
            .push(PrInfo {
                number,
                title,
                author,
                branch,
                web_url: pr_web_url,
            });
    }

    for (full_name, web_url, _identity) in &all_repos {
        let repo_prs = prs_by_repo.remove(full_name).unwrap_or_default();
        let open_pr_count = repo_prs.len() as u32;

        for pi in &repo_prs {
            pulls.push(OpenPull {
                provider: "bitbucket-server".into(),
                full_name: full_name.clone(),
                number: pi.number,
                title: pi.title.clone(),
                author_display: pi.author.clone(),
                is_draft: false,
                branch: pi.branch.clone(),
                web_url: pi.web_url.clone(),
                account_id: acct.id.clone(),
                account_label: acct.label.clone(),
            });
        }

        // Bitbucket Server has no CI pipeline API — always "unknown"
        let short_sha = String::new();
        inv.repos.push(RepoSummary {
            full_name: full_name.clone(),
            provider: "bitbucket-server".into(),
            last_state: "unknown".into(),
            open_prs: open_pr_count,
            default_branch_sha: short_sha,
            default_branch: "main".into(),
            web_url: web_url.clone(),
        });

    }

    inv.open_pulls = pulls;
    inv.last_sync_ms = now_ms();
    inv.last_error = String::new();
    Ok(inv)
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
