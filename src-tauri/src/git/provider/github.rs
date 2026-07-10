//! GitHub provider — one GraphQL call to `/graphql` covering repos +
//! latest CI state + open PRs. Minimum API surface.
//!
//! Token: classic PAT or fine-grained PAT with `metadata:read` +
//! `pull_requests:read` + `actions:read` (Actions status comes
//! embedded in `Status.contexts.state`).

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
        defaultBranchRef { name }
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
        ref(qualifiedName: "refs/heads/__default__") {
          target {
            ... on Commit {
              oid
              status {
                state
                contexts { state targetUrl }
              }
            }
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
    let mut inv = AcctInventory::empty(acct);
    let mut failed = Vec::new();
    let mut pulls = Vec::new();
    let mut open_pr_total = 0u32;
    let repos = v
        .pointer("/data/viewer/repositories/nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
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
        let (last_state, short_sha) = ref_state(&repo);
        inv.repos.push(RepoSummary {
            full_name: full_name.clone(),
            provider: "github".into(),
            last_state: last_state.clone(),
            open_prs,
            default_branch_sha: short_sha,
            default_branch,
            web_url,
        });
        if last_state == "failed" || last_state == "cancelled" {
            // For GitHub we surface ALL "failed"/"cancelled" refs as FailRun
            // entries — there's no per-run table in the GraphQL response so
            // the chip count is "number of repos whose latest default-branch
            // CI failed / cancelled" rather than total run count. This is per
            // the user's "actions that fail" intent: one chip per broken repo.
            failed.push(FailRun {
                provider: "github".into(),
                full_name: full_name.clone(),
                run_label: "latest default-branch CI".into(),
                branch: "(default)".into(),
                short_sha: String::new(),
                ago: String::new(),
                finished_ms: 0,
                web_url: String::new(),
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

fn ref_state(repo: &serde_json::Value) -> (String, String) {
    let target = repo.pointer("/ref/target");
    let oid = target
        .and_then(|t| t.pointer("/oid"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    let short = oid.chars().take(7).collect::<String>();
    let state = target
        .and_then(|t| t.pointer("/status/state"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown");
    let mapped = match state {
        "SUCCESS" => "success",
        "FAILURE" | "ERROR" => "failed",
        "PENDING" => "running",
        "EXPECTED" => "running",
        _ => "unknown",
    };
    (mapped.into(), short)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
