import "../../styles/globals.css";
import "./git-manager.css";
import { mountWindow } from "../../shared/window";
import { mountTabs } from "../../shared/tabs";
import { mountFilterPills } from "../../shared/filter-pills";
import { setIcon } from "../../shared/icon";
import { initLog, logInfo, logMemory } from "../../shared/log";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CMD } from "../../shared/ipc";
import { EVENT } from "../../shared/events";
import type {
  AcctInventory,
  FailRun,
  GitState,
  GitWidgetConfig,
  OpenPull,
  RepoSummary,
} from "../../shared/types";

interface GitManagerGlobals {
  __ZENITH_GIT_ACCOUNT_ID: string | null;
}

type AcctFilter = "all" | string;

void (async () => {
  await initLog();
  logMemory("startup");

  let state: GitState = { inventories: [], total_failed: 0, total_open_prs: 0 };
  let cfg: GitWidgetConfig = { accounts: [], selected_account_id: null, poll_interval_mins: 5 };
  let acctFilter: AcctFilter =
    ((window as unknown as Partial<GitManagerGlobals>).__ZENITH_GIT_ACCOUNT_ID as string | null) ??
    "all";
  if (acctFilter === "") acctFilter = "all";

  // ---- chrome ---------------------------------------------------------------
  const { content } = await mountWindow({ title: "Git Manager" });

  // ---- account selector (filter pills) ------------------------------------
  const pillsWrap = document.createElement("div");
  pillsWrap.className = "gm-toolbar";
  content.prepend(pillsWrap);
  const pills = mountFilterPills<AcctFilter>(
    pillsWrap,
    [{ id: "all", label: "All" }],
    "all",
  );
  pills.container.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-pill-id]");
    if (!btn) return;
    const next = btn.dataset.pillId as AcctFilter;
    if (next === acctFilter) return;
    acctFilter = next;
    for (const p of pills.container.querySelectorAll<HTMLButtonElement>("[data-pill-id]")) {
      p.classList.toggle("is-active", p.dataset.pillId === next);
    }
    render();
  });

  // ---- tabs ----------------------------------------------------------------
  const tabs = mountTabs(content, [
    { id: "failed", label: "Failed CI" },
    { id: "prs", label: "Open PRs" },
    { id: "overview", label: "Overview" },
  ]);
  content.prepend(tabs.container);

  // ---- refresh button (footer-ish; mount into content below the panes) ------
  const footer = document.createElement("div");
  footer.className = "gm-footer";
  content.append(footer);

  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "zen-button is-outline is-sm";
  refreshBtn.textContent = "Refresh";
  refIcon(refreshBtn, "refresh-cw", 14, "Refresh now");
  refreshBtn.addEventListener("click", () => {
    void invoke(CMD.gitRefresh).then(() => void refresh());
  });
  footer.append(refreshBtn);

  const meta = document.createElement("span");
  meta.className = "gm-meta";
  footer.append(meta);

  // ---- data + render -------------------------------------------------------
  try {
    cfg = await invoke<GitWidgetConfig>(CMD.getGitWidgetConfig);
  } catch (_) {
    /* fall back to defaults */
  }
  try {
    state = await invoke<GitState>(CMD.getGitState, { accountId: null });
  } catch (_) {
    /* state stays empty */
  }
  rebuildAccountPills();
  render();

  // Live updates from the poll thread so the window refreshes automatically
  // — the user doesn't need to click Refresh.
  listen<GitState>(EVENT.gitChanged, (e) => {
    state = e.payload;
    render();
  });
  listen<GitState>("zenith:config-updated", async () => {
    try { cfg = await invoke<GitWidgetConfig>(CMD.getGitWidgetConfig); }
    catch (_) { /* ignore */ }
    rebuildAccountPills();
    render();
  });

  logMemory("after mount");
  logInfo("git-manager ready");

  function refresh() {
    void invoke(CMD.getGitState, { accountId: null }).then((s) => {
      state = s as GitState;
      render();
    });
  }

  function rebuildAccountPills() {
    // Remove existing account pills, keep "All".
    const ids = new Set(cfg.accounts.map((a) => a.id));
    for (const btn of Array.from(
      pills.container.querySelectorAll<HTMLButtonElement>("[data-pill-id]"),
    )) {
      if (btn.dataset.pillId === "all") continue;
      if (!ids.has(btn.dataset.pillId as string)) btn.remove();
    }
    for (const a of cfg.accounts) {
      if (pills.container.querySelector(`[data-pill-id="${cssEscape(a.id)}"]`)) continue;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "zen-filter-pill";
      btn.dataset.pillId = a.id;
      btn.textContent = a.label || a.username || a.provider;
      pills.container.append(btn);
    }
    if (acctFilter !== "all" && !ids.has(acctFilter)) acctFilter = "all";
    for (const p of pills.container.querySelectorAll<HTMLButtonElement>("[data-pill-id]")) {
      p.classList.toggle("is-active", p.dataset.pillId === acctFilter);
    }
  }

  function filtered(): AcctInventory[] {
    if (acctFilter === "all") return state.inventories;
    return state.inventories.filter((i) => i.account_id === acctFilter);
  }

  function render() {
    renderFailed(tabs.panes.failed);
    renderPrs(tabs.panes.prs);
    renderOverview(tabs.panes.overview);
    paintMeta();
  }

  function renderFailed(pane: HTMLElement) {
    pane.textContent = "";
    const invs = filtered();
    const runs: FailRun[] = [];
    for (const inv of invs) runs.push(...inv.failed_runs);
    if (runs.length === 0) {
      pane.append(emptyState("No failed CI runs", "All clear across the selected accounts."));
      return;
    }
    const list = document.createElement("div");
    list.className = "gm-list";
    for (const r of runs) list.append(failCard(r));
    pane.append(list);
  }

  function renderPrs(pane: HTMLElement) {
    pane.textContent = "";
    const invs = filtered();
    const prs: OpenPull[] = [];
    for (const inv of invs) prs.push(...inv.open_pulls);
    if (prs.length === 0) {
      pane.append(emptyState("No open PRs / MRs", "Nothing awaiting review right now."));
      return;
    }
    const list = document.createElement("div");
    list.className = "gm-list";
    for (const p of prs) list.append(prCard(p));
    pane.append(list);
  }

  function renderOverview(pane: HTMLElement) {
    pane.textContent = "";
    const invs = filtered();
    const repos: RepoSummary[] = [];
    for (const inv of invs) repos.push(...inv.repos);
    if (repos.length === 0) {
      const errInvs = invs.filter((i) => i.last_error && i.last_error.length > 0);
      if (errInvs.length > 0) {
        const errs = errInvs.map(
          (i) => `${i.account_label || i.username || i.provider}: ${i.last_error}`,
        );
        pane.append(
          emptyState("Account error", errs.join("\n")),
        );
        return;
      }
      pane.append(
        emptyState(
          "No repos yet",
          acctFilter === "all"
            ? "Add an account via the gear button on the Git widget in the Widget Manager."
            : "This account has no repos the token can read.",
        ),
      );
      return;
    }
    const list = document.createElement("div");
    list.className = "gm-list";
    for (const r of repos) list.append(repoCard(r));
    pane.append(list);
  }

  function paintMeta() {
    const invs = filtered();
    const anyErr = invs.some((i) => i.last_error && i.last_error.length > 0);
    const fresh = invs.filter((i) => i.last_sync_ms > 0);
    const lastMs = fresh.reduce((m, i) => Math.max(m, i.last_sync_ms), 0);
    const ageLabel = lastMs > 0 ? relAge(lastMs) : "never";
    const counts = `${state.total_failed} failed · ${state.total_open_prs} open PRs`;
    meta.textContent = "";
    if (anyErr) {
      const warn = document.createElement("span");
      warn.className = "gm-warn";
      warn.textContent = "Some accounts failed — see overview.";
      meta.append(warn, document.createTextNode("  ·  "));
    }
    meta.append(document.createTextNode(`${counts}  ·  synced ${ageLabel}`));
  }

  // ---- cards ----------------------------------------------------------------
  function failCard(r: FailRun): HTMLElement {
    const card = document.createElement("article");
    card.className = "zen-card gm-fail";
    card.tabIndex = 0;
    card.dataset.url = r.web_url;
    card.addEventListener("click", () => void openCardUrl(r.web_url));
    const head = document.createElement("header");
    head.className = "zen-card__header";
    const stateChip = document.createElement("span");
    stateChip.className = "gm-state-chip is-" + r.provider;
    stateChip.textContent = r.run_label || "failed";
    const title = document.createElement("span");
    title.className = "zen-card__title";
    title.textContent = r.full_name;
    head.append(stateChip, title);
    if (r.account_label) {
      const acct = document.createElement("span");
      acct.className = "gm-account";
      acct.textContent = r.account_label;
      head.append(acct);
    }
    card.append(head);
    if (r.branch || r.short_sha || r.ago) {
      const body = document.createElement("div");
      body.className = "zen-card__content gm-detail";
      if (r.branch) body.append(detailLine("branch", r.branch));
      if (r.short_sha) body.append(detailLine("sha", r.short_sha));
      if (r.ago) body.append(detailLine("when", r.ago));
      card.append(body);
    }
    return card;
  }

  function prCard(p: OpenPull): HTMLElement {
    const card = document.createElement("article");
    card.className = "zen-card gm-pr";
    card.tabIndex = 0;
    card.dataset.url = p.web_url;
    card.addEventListener("click", () => void openCardUrl(p.web_url));
    const head = document.createElement("header");
    head.className = "zen-card__header";
    const chip = document.createElement("span");
    chip.className = "gm-state-chip is-" + p.provider;
    chip.textContent = "#" + p.number;
    const title = document.createElement("span");
    title.className = "zen-card__title";
    title.textContent = p.title;
    head.append(chip, title);
    if (p.is_draft) {
      const draft = document.createElement("span");
      draft.className = "gm-draft";
      draft.textContent = "draft";
      head.append(draft);
    }
    card.append(head);
    const body = document.createElement("div");
    body.className = "zen-card__content gm-detail";
    body.append(detailLine("repo", p.full_name));
    body.append(detailLine("by", p.author_display));
    if (p.branch) body.append(detailLine("branch", p.branch));
    card.append(body);
    return card;
  }

  function repoCard(r: RepoSummary): HTMLElement {
    const card = document.createElement("article");
    card.className = "zen-card gm-repo";
    card.tabIndex = 0;
    card.dataset.url = r.web_url;
    card.addEventListener("click", () => void openCardUrl(r.web_url));
    const head = document.createElement("header");
    head.className = "zen-card__header";
    const ttl = document.createElement("span");
    ttl.className = "zen-card__title";
    ttl.textContent = r.full_name;
    head.append(ttl);
    head.append(stateDot(r.last_state));
    if (r.open_prs > 0) {
      const prs = document.createElement("span");
      prs.className = "gm-pr-count";
      prs.textContent = String(r.open_prs) + " PRs";
      head.append(prs);
    }
    card.append(head);
    if (r.default_branch || r.default_branch_sha) {
      const body = document.createElement("div");
      body.className = "zen-card__content gm-detail";
      if (r.default_branch) body.append(detailLine("branch", r.default_branch));
      if (r.default_branch_sha) body.append(detailLine("sha", r.default_branch_sha));
      card.append(body);
    }
    return card;
  }

  function detailLine(label: string, value: string): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "gm-dl";
    const k = document.createElement("span");
    k.className = "gm-dl-k";
    k.textContent = label;
    const v = document.createElement("span");
    v.className = "gm-dl-v";
    v.textContent = value;
    wrap.append(k, v);
    return wrap;
  }

  function stateDot(state: string): HTMLElement {
    const d = document.createElement("span");
    d.className = "gm-state-dot is-" + state;
    d.setAttribute("aria-label", state);
    return d;
  }

  function emptyState(title: string, hint: string): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "gm-empty";
    const t = document.createElement("div");
    t.className = "gm-empty-title";
    t.textContent = title;
    const h = document.createElement("p");
    h.className = "zen-hint";
    h.textContent = hint;
    wrap.append(t, h);
    return wrap;
  }

  async function openCardUrl(url: string) {
    if (!url) return;
    try {
      await invoke(CMD.openUrl, { url });
    } catch {
      window.open(url, "_blank", "noopener");
    }
  }
})();

function refIcon(btn: HTMLButtonElement, name: string, size: number, title: string) {
  const ic = document.createElement("i");
  ic.dataset.icon = name;
  ic.dataset.size = String(size);
  ic.setAttribute("aria-hidden", "true");
  if (title) btn.title = title;
  btn.prepend(ic);
  setIcon(ic, name, { size });
}

function relAge(ms: number): string {
  const delta = Date.now() - ms;
  if (delta < 0) return "now";
  const s = Math.round(delta / 1000);
  if (s < 60) return s + "s ago";
  const mins = Math.round(s / 60);
  if (mins < 60) return mins + "m ago";
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return hrs + "h ago";
  const days = Math.round(hrs / 24);
  return days + "d ago";
}

function cssEscape(s: string): string {
  if (window.CSS && typeof CSS.escape === "function") return CSS.escape(s);
  return s.replace(/["\\\n]/g, "\\$&");
}
