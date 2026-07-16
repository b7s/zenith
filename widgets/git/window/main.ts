import "../../../src/styles/globals.css";
import "./git-manager.css";
import { mountWindow } from "../../../src/shared/window";
import { mountTabs } from "../../../src/shared/tabs";
import { mountFilterPills } from "../../../src/shared/filter-pills";
import { setIcon } from "../../../src/shared/icon";
import { initLog, logInfo, logMemory } from "../../../src/shared/log";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CMD } from "../../../src/shared/ipc";
import { EVENT } from "../../../src/shared/events";
import type {
  AcctInventory,
  Config,
  FailRun,
  GitState,
  GitWidgetConfig,
  OpenPull,
  RepoSummary,
} from "../../../src/shared/types";

interface GitManagerGlobals {
  __ZENITH_GIT_ACCOUNT_ID: string | null;
}

type AcctFilter = "all" | string;

void (async () => {
  await initLog();
  logMemory("startup");

  let state: GitState = { inventories: [], total_failed: 0, total_open_prs: 0 };
  let cfg: GitWidgetConfig = { accounts: [], selected_account_id: null, poll_interval_mins: 5 };
  // AI assistants the user enabled in the git widget config — drives the
  // per-card "Send to AI" dropdown.
  let aiClis: string[] = [];
  let acctFilter: AcctFilter =
    ((window as unknown as Partial<GitManagerGlobals>).__ZENITH_GIT_ACCOUNT_ID as string | null) ??
    "all";
  if (acctFilter === "") acctFilter = "all";

  // ---- chrome ---------------------------------------------------------------
  const { content, titleBadge } = await mountWindow({ title: "Git Manager" });

  // ---- account selector (filter pills) ------------------------------------
  // Lives in the header, right after the title (same left alignment).
  const pillsWrap = document.createElement("div");
  pillsWrap.className = "gm-toolbar";
  titleBadge?.parentElement?.append(pillsWrap);
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
    { id: "dashboard", label: "Dashboard" },
    { id: "failed", label: "Failed CI" },
    { id: "prs", label: "Open PRs" },
    { id: "overview", label: "Overview" },
  ]);

  // Right-aligned action — mirrors the Settings window "Widgets" link style.
  // The icon lives in its own <span class="zen-icon"> child: calling setIcon
  // on the button itself would add `.zen-icon` (which has `contain: strict`
  // size containment) to the button and collapse it to zero width.
  const configLink = document.createElement("button");
  configLink.type = "button";
  configLink.className = "zen-tab zen-tab--action";
  configLink.title = "Configure Git widget";
  configLink.setAttribute("aria-label", "Configure Git widget");
  const configIcon = document.createElement("span");
  configIcon.className = "zen-icon";
  setIcon(configIcon, "config", { size: 14 });
  configLink.append(configIcon);
  configLink.addEventListener("click", () =>
    void invoke(CMD.openWidgetConfig, { widgetId: "git" }),
  );
  tabs.container.append(configLink);

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
    refreshBtn.classList.add("is-loading");
    refreshBtn.disabled = true;
    void invoke(CMD.gitRefresh);
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
  try {
    const full = await invoke<Config>(CMD.getConfig);
    const list = full.widgets?.config?.git?.ai_clis;
    if (Array.isArray(list)) aiClis = list.map(String).filter(Boolean);
  } catch (_) {
    /* aiClis stays empty */
  }
  rebuildAccountPills();
  render();

  // Live updates from the poll thread so the window refreshes automatically
  // — the user doesn't need to click Refresh.
  listen<GitState>(EVENT.gitChanged, (e) => {
    state = e.payload;
    rebuildAccountPills();
    render();
    refreshBtn.classList.remove("is-loading");
    refreshBtn.disabled = false;
  });
  listen<GitState>("zenith:config-updated", async () => {
    try { cfg = await invoke<GitWidgetConfig>(CMD.getGitWidgetConfig); }
    catch (_) { /* ignore */ }
    try {
      const full = await invoke<Config>(CMD.getConfig);
      const list = full.widgets?.config?.git?.ai_clis;
      if (Array.isArray(list)) aiClis = list.map(String).filter(Boolean);
    } catch (_) { /* ignore */ }
    rebuildAccountPills();
    render();
  });

  logMemory("after mount");
  logInfo("git-manager ready");


  function rebuildAccountPills() {
    const errIds = new Set(
      state.inventories
        .filter((i) => i.last_error && i.last_error.length > 0)
        .map((i) => i.account_id),
    );
    const ids = new Set(cfg.accounts.map((a) => a.id));
    for (const btn of Array.from(
      pills.container.querySelectorAll<HTMLButtonElement>("[data-pill-id]"),
    )) {
      if (btn.dataset.pillId === "all") continue;
      if (!ids.has(btn.dataset.pillId as string)) {
        btn.remove();
        continue;
      }
      updateErrDot(btn, errIds.has(btn.dataset.pillId as string));
    }
    for (const a of cfg.accounts) {
      if (pills.container.querySelector(`[data-pill-id="${cssEscape(a.id)}"]`)) continue;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "zen-filter-pill";
      btn.dataset.pillId = a.id;
      btn.textContent = a.label || a.username || a.provider;
      updateErrDot(btn, errIds.has(a.id));
      pills.container.append(btn);
    }
    if (acctFilter !== "all" && !ids.has(acctFilter)) acctFilter = "all";
    for (const p of pills.container.querySelectorAll<HTMLButtonElement>("[data-pill-id]")) {
      p.classList.toggle("is-active", p.dataset.pillId === acctFilter);
    }
  }

  function updateErrDot(btn: HTMLButtonElement, hasErr: boolean): void {
    const existing = btn.querySelector(".gm-err-dot");
    if (hasErr && !existing) {
      const dot = document.createElement("span");
      dot.className = "gm-err-dot";
      btn.append(dot);
    } else if (!hasErr && existing) {
      existing.remove();
    }
  }

  function filtered(): AcctInventory[] {
    if (acctFilter === "all") return state.inventories;
    return state.inventories.filter((i) => i.account_id === acctFilter);
  }

  function render() {
    renderDashboard(tabs.panes.dashboard);
    renderFailed(tabs.panes.failed);
    renderPrs(tabs.panes.prs);
    renderOverview(tabs.panes.overview);
    paintMeta();
  }

  function renderDashboard(pane: HTMLElement) {
    pane.textContent = "";
    const invs = filtered();

    const repos: RepoSummary[] = [];
    const runs: FailRun[] = [];
    let acctErr = 0;
    for (const inv of invs) {
      repos.push(...inv.repos);
      runs.push(...inv.failed_runs);
      if (inv.last_error && inv.last_error.length > 0) acctErr++;
    }
    const failedRepos = new Set(runs.map((r) => r.full_name));

    const stateCounts: Record<string, number> = {
      success: 0,
      failed: 0,
      running: 0,
      cancelled: 0,
      unknown: 0,
    };
    for (const r of repos) {
      let k = ["success", "failed", "running", "cancelled", "unknown"].includes(r.last_state)
        ? r.last_state
        : "unknown";
      if (k === "failed" && !failedRepos.has(r.full_name)) k = "unknown";
      stateCounts[k]++;
    }
    const attention = repos.filter(
      (r) => r.last_state === "failed" || r.last_state === "running" || r.last_state === "cancelled",
    );
    const brokenAccts = invs.filter((i) => i.last_error && i.last_error.length > 0);

    const dash = document.createElement("div");
    dash.className = "gm-dash";

    // --- At a glance --------------------------------------------------------
    const glance = document.createElement("section");
    glance.className = "gm-section";
    glance.append(sectionTitle("At a glance"));
    const tiles = document.createElement("div");
    tiles.className = "gm-tiles";

    const acctTotal = cfg.accounts.length;
    tiles.append(
      statTile(String(state.total_failed), "Failed CI", state.total_failed > 0 ? "is-danger" : "", () => tabs.switchTo("failed")),
      statTile(String(state.total_open_prs), "Open PRs", state.total_open_prs > 0 ? "is-info" : "", () => tabs.switchTo("prs")),
      statTile(String(repos.length), "Repos tracked", ""),
      statTile(
        `${acctTotal}`,
        acctErr > 0 ? `Accounts · ${acctErr} error${acctErr > 1 ? "s" : ""}` : "Accounts",
        acctErr > 0 ? "is-danger" : "",
      ),
    );
    glance.append(tiles);
    dash.append(glance);

    // --- CI status (stacked bar) -------------------------------------------
    const ci = document.createElement("section");
    ci.className = "gm-section";
    ci.append(sectionTitle("CI status"));
    const totalStates = repos.length || 1;
    const stack = document.createElement("div");
    stack.className = "gm-stackbar";
    const order: Array<[string, string]> = [
      ["success", "Success"],
      ["failed", "Failed"],
      ["running", "Running"],
      ["cancelled", "Cancelled"],
      ["unknown", "Others"],
    ];
    for (const [k, label] of order) {
      const c = stateCounts[k];
      if (c === 0) continue;
      const seg = document.createElement("span");
      seg.className = `gm-stack-seg is-${k}`;
      seg.style.width = `${(c / totalStates) * 100}%`;
      seg.title = `${label}: ${c}`;
      stack.append(seg);
    }
    ci.append(stack);
    const legend = document.createElement("div");
    legend.className = "gm-legend";
    for (const [k, label] of order) {
      const item = document.createElement("span");
      item.className = "gm-legend-item";
      const dot = document.createElement("span");
      dot.className = `gm-legend-dot is-${k}`;
      item.append(dot, document.createTextNode(`${label} ${stateCounts[k]}`));
      legend.append(item);
    }
    ci.append(legend);
    dash.append(ci);

    // --- Failures by day (bar chart) ---------------------------------------
    const fd = document.createElement("section");
    fd.className = "gm-section";
    const winDays = cfg.failures_window_days ?? 14;
    const DAY_MS = 24 * 3600 * 1000;
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    let DAYS: number;
    if (winDays > 0) {
      DAYS = winDays;
    } else {
      let earliest = today.getTime();
      for (const r of runs) {
        if (r.finished_ms > 0 && r.finished_ms < earliest) earliest = r.finished_ms;
      }
      const span = runs.length > 0 ? Math.ceil((today.getTime() - earliest) / DAY_MS) + 1 : 7;
      DAYS = Math.max(7, Math.min(span, 90));
    }
    fd.append(sectionTitle(`Failed runs by day (${winDays > 0 ? winDays + "d" : "90d"})`));
    const buckets: { label: string; count: number; ts: number }[] = [];
    for (let i = DAYS - 1; i >= 0; i--) {
      const d = new Date(today);
      d.setDate(today.getDate() - i);
      buckets.push({ label: `${d.getMonth() + 1}/${d.getDate()}`, count: 0, ts: d.getTime() });
    }
    const startTs = buckets[0].ts;
    const endTs = today.getTime() + DAY_MS;
    for (const r of runs) {
      if (r.finished_ms >= startTs && r.finished_ms <= endTs) {
        const d = new Date(r.finished_ms);
        d.setHours(0, 0, 0, 0);
        const idx = Math.round((d.getTime() - startTs) / DAY_MS);
        if (idx >= 0 && idx < DAYS) buckets[idx].count++;
      }
    }
    const maxCount = Math.max(1, ...buckets.map((b) => b.count));
    const chart = document.createElement("div");
    chart.className = "gm-barchart";
    for (const b of buckets) {
      const col = document.createElement("div");
      col.className = "gm-bar-col";
      const bar = document.createElement("span");
      bar.className = "gm-bar";
      bar.style.height = `${(b.count / maxCount) * 100}%`;
      bar.title = `${b.label}: ${b.count}`;
      const lbl = document.createElement("span");
      lbl.className = "gm-bar-label";
      lbl.textContent = b.label;
      col.append(bar, lbl);
      chart.append(col);
    }
    fd.append(chart);
    dash.append(fd);

    // --- Needs attention ----------------------------------------------------
    const att = document.createElement("section");
    att.className = "gm-section";
    att.append(sectionTitle("Needs attention"));
    if (attention.length === 0 && brokenAccts.length === 0) {
      att.append(
        emptyState("All healthy", "No repos in a failed, running, or cancelled state."),
      );
    } else {
      const list = document.createElement("div");
      list.className = "gm-list";
      for (const r of attention) list.append(repoChip(r));
      for (const a of brokenAccts) {
        const card = document.createElement("article");
        card.className = "zen-card gm-repo is-broken";
        const head = document.createElement("header");
        head.className = "zen-card__header";
        const ttl = document.createElement("span");
        ttl.className = "zen-card__title";
        ttl.textContent = a.account_label || a.username || a.provider;
        head.append(ttl, stateDot("failed"));
        card.append(head);
        const body = document.createElement("div");
        body.className = "zen-card__content gm-detail";
        body.append(detailLine("error", a.last_error));
        card.append(body);
        list.append(card);
      }
      att.append(list);
    }
    dash.append(att);

    pane.append(dash);
  }

  function sectionTitle(text: string): HTMLElement {
    const h = document.createElement("h3");
    h.className = "gm-section-title";
    h.textContent = text;
    return h;
  }

  function statTile(value: string, label: string, mod: string, onClick?: () => void): HTMLElement {
    const tile = document.createElement("div");
    tile.className = "gm-tile" + (mod ? " " + mod : "");
    if (onClick) {
      tile.tabIndex = 0;
      tile.classList.add("gm-tile--clickable");
      tile.setAttribute("role", "button");
      const handler = (e: Event) => {
        if (e instanceof KeyboardEvent && e.key !== "Enter" && e.key !== " ") return;
        e.preventDefault();
        onClick();
      };
      tile.addEventListener("click", handler);
      tile.addEventListener("keydown", handler);
    }
    const v = document.createElement("span");
    v.className = "gm-tile-val";
    v.textContent = value;
    const l = document.createElement("span");
    l.className = "gm-tile-label";
    l.textContent = label;
    tile.append(v, l);
    return tile;
  }

  function repoChip(r: RepoSummary): HTMLElement {
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
    head.append(ttl, stateDot(r.last_state));
    if (r.open_prs > 0) {
      const prs = document.createElement("span");
      prs.className = "gm-pr-count";
      prs.textContent = String(r.open_prs) + " PRs";
      head.append(prs);
    }
    card.append(head);
    return card;
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
    const errInvs = invs.filter((i) => i.last_error && i.last_error.length > 0);

    // Always show errored accounts at the top, even when other accounts have
    // repos — otherwise a silent error (auth failure, rate limit, etc.) on
    // one account is invisible as long as any other account succeeds.
    if (errInvs.length > 0) {
      const errBlock = document.createElement("div");
      errBlock.className = "gm-err-block";
      errBlock.style.cssText = "margin-bottom:0.75rem;display:flex;flex-direction:column;gap:0.35rem;";
      for (const i of errInvs) {
        const row = document.createElement("div");
        row.className = "gm-err-row";
        row.style.cssText = "display:flex;align-items:center;gap:0.5rem;padding:0.5rem;border-radius:var(--radius);background:color-mix(in oklch,var(--danger) 12%,transparent);border:1px solid color-mix(in oklch,var(--danger) 30%,transparent);font-size:0.8125rem;";
        const labelSpan = document.createElement("span");
        labelSpan.style.cssText = "font-weight:600;flex-shrink:0;";
        labelSpan.textContent = i.account_label || i.username || i.provider;
        const msgSpan = document.createElement("span");
        msgSpan.style.cssText = "color:var(--muted-foreground);";
        msgSpan.textContent = i.last_error;
        row.append(labelSpan, msgSpan);
        errBlock.append(row);
      }
      pane.append(errBlock);
    }

    if (repos.length === 0 && errInvs.length === 0) {
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

    if (repos.length > 0) {
      const list = document.createElement("div");
      list.className = "gm-list";
      for (const r of repos) list.append(repoCard(r));
      pane.append(list);
    }
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
  function providerLabel(provider: string): string {
    if (provider === "github") return "GitHub";
    if (provider === "gitlab") return "GitLab";
    return provider.charAt(0).toUpperCase() + provider.slice(1);
  }

  function failCard(r: FailRun): HTMLElement {
    const card = document.createElement("article");
    card.className = "zen-card gm-fail";
    card.tabIndex = 0;
    card.dataset.url = r.web_url;
    card.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).closest(".gm-ai-btn, .gm-ai-menu")) return;
      void openCardUrl(r.web_url);
    });

    const accent = document.createElement("span");
    accent.className = "gm-card-accent";
    card.append(accent);

    const head = document.createElement("header");
    head.className = "zen-card__header gm-card-head";

    const main = document.createElement("div");
    main.className = "gm-card-main";

    const titleRow = document.createElement("div");
    titleRow.className = "gm-card-titlerow";
    const pill = document.createElement("span");
    pill.className = "gm-status-pill is-fail";
    pill.textContent = "FAILED";
    const title = document.createElement("span");
    title.className = "zen-card__title";
    title.textContent = r.run_label ? `${r.run_label} · ${r.full_name}` : r.full_name;
    titleRow.append(pill, title);
    main.append(titleRow);

    const sub = document.createElement("div");
    sub.className = "gm-card-sub";
    const prov = document.createElement("span");
    prov.className = "gm-provider is-" + r.provider;
    prov.textContent = providerLabel(r.provider);
    sub.append(prov);
    main.append(sub);
    head.append(main, attachAiButton(card, () => failPrompt(r), () => runCopyContent(r)));
    card.append(head);

    if (r.branch || r.short_sha || r.ago || r.account_label) {
      const body = document.createElement("div");
      body.className = "zen-card__content gm-detail";
      if (r.branch && r.short_sha) {
        body.append(detailLinePair("branch", r.branch, "sha", r.short_sha));
      } else {
        if (r.branch) body.append(detailLine("branch", r.branch));
        if (r.short_sha) body.append(detailLine("sha", r.short_sha));
      }
      if (r.ago) body.append(detailLine("when", r.ago));
      if (r.account_label) {
        const acct = document.createElement("span");
        acct.className = "gm-account gm-detail-account";
        acct.textContent = r.account_label;
        body.append(acct);
      }
      card.append(body);
    }
    return card;
  }

  function prCard(p: OpenPull): HTMLElement {
    const card = document.createElement("article");
    card.className = "zen-card gm-pr";
    card.tabIndex = 0;
    card.dataset.url = p.web_url;
    card.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).closest(".gm-ai-btn, .gm-ai-menu")) return;
      void openCardUrl(p.web_url);
    });

    const accent = document.createElement("span");
    accent.className = "gm-card-accent";
    card.append(accent);

    const head = document.createElement("header");
    head.className = "zen-card__header gm-card-head";

    const main = document.createElement("div");
    main.className = "gm-card-main";

    const titleRow = document.createElement("div");
    titleRow.className = "gm-card-titlerow";
    const num = document.createElement("span");
    num.className = "gm-pr-num";
    num.textContent = "#" + p.number;
    const title = document.createElement("span");
    title.className = "zen-card__title";
    title.textContent = p.title;
    titleRow.append(num, title);
    if (p.is_draft) {
      const draft = document.createElement("span");
      draft.className = "gm-status-pill is-draft";
      draft.textContent = "DRAFT";
      titleRow.append(draft);
    }
    main.append(titleRow);

    const sub = document.createElement("div");
    sub.className = "gm-card-sub";
    const prov = document.createElement("span");
    prov.className = "gm-provider is-" + p.provider;
    prov.textContent = providerLabel(p.provider);
    sub.append(prov);
    main.append(sub);
    head.append(main, attachAiButton(card, () => prPrompt(p), () => prCopyContent(p)));
    card.append(head);

    const body = document.createElement("div");
    body.className = "zen-card__content gm-detail";
    body.append(detailLine("repo", p.full_name));
    body.append(detailLine("by", p.author_display));
    if (p.branch) body.append(detailLine("branch", p.branch));
    if (p.account_label) {
      const acct = document.createElement("span");
      acct.className = "gm-account gm-detail-account";
      acct.textContent = p.account_label;
      body.append(acct);
    }
    card.append(body);
    return card;
  }

  function attachAiButton(
    _card: HTMLElement,
    getPrompt: () => string,
    getCopyContent: () => Promise<string>,
  ): HTMLButtonElement {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "gm-ai-btn";
    btn.title = "Send to AI assistant";
    btn.setAttribute("aria-label", "Send to AI assistant");
    const ic = document.createElement("span");
    ic.className = "zen-icon";
    setIcon(ic, "sparkles", { size: 14 });
    btn.append(ic);

    let menu: HTMLElement | null = null;

    function closeMenu() {
      menu?.remove();
      menu = null;
      document.removeEventListener("click", onDocClick, true);
      window.removeEventListener("scroll", closeMenu, true);
      window.removeEventListener("resize", closeMenu, true);
    }
    function onDocClick(ev: MouseEvent) {
      if (menu && !menu.contains(ev.target as Node) && ev.target !== btn) closeMenu();
    }

    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      if (aiClis.length === 0) {
        // Nothing configured yet — send them to the widget settings.
        void invoke(CMD.openWidgetConfig, { widgetId: "git" });
        return;
      }
      if (menu) {
        closeMenu();
        return;
      }
      menu = document.createElement("div");
      menu.className = "gm-ai-menu";

      // First option: copy the real failure/PR content (fetched lazily so we
      // never preload heavy logs/diffs for every card). A spinner stands in
      // for the icon while the fetch is in flight.
      const copyItem = document.createElement("button");
      copyItem.type = "button";
      copyItem.className = "gm-ai-item gm-ai-copy";
      const copyIc = document.createElement("span");
      copyIc.className = "zen-icon";
      setIcon(copyIc, "copy", { size: 14 });
      const copyLabel = document.createElement("span");
      copyLabel.textContent = "Copy content";
      copyItem.append(copyIc, copyLabel);
      let copying = false;
      copyItem.addEventListener("click", async (ev) => {
        ev.stopPropagation();
        if (copying) return;
        copying = true;
        setIcon(copyIc, "loader", { size: 14 });
        copyIc.classList.add("gm-spin");
        try {
          const text = await getCopyContent();
          await navigator.clipboard.writeText(text);
          copyLabel.textContent = "Copied ✓";
        } catch {
          copyLabel.textContent = "Copy failed";
        }
        copying = false;
        setTimeout(() => closeMenu(), 750);
      });
      menu.append(copyItem);

      const sep = document.createElement("div");
      sep.className = "gm-ai-sep";
      menu.append(sep);

      for (const cli of aiClis) {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "gm-ai-item";
        item.textContent = cli;
        item.addEventListener("click", (ev) => {
          ev.stopPropagation();
          const prompt = getPrompt();
          void invoke(CMD.sendToAi, { cli, prompt })
            .catch((err) => {
              const msg = `[git-manager] sendToAi failed for ${cli}: ${err}`;
              console.error(msg);
              void invoke("log_write", { window: "git-manager", level: "ERROR", message: msg });
              // Surface a clean dialog instead of a silent log — the backend
              // returns a friendly error when the CLI is missing (no flashing
              // console), so show that to the user.
              void invoke(CMD.showDialog, {
                spec: { kind: "message", data: { title: "AI assistant", body: String(err) } },
              });
            });
          closeMenu();
        });
        menu.append(item);
      }
      // Fixed-positioned on <body> so the pane's overflow:auto can't clip it.
      const rect = btn.getBoundingClientRect();
      menu.style.position = "fixed";
      menu.style.top = `${rect.bottom + 4}px`;
      menu.style.right = `${Math.max(8, window.innerWidth - rect.right)}px`;
      document.body.append(menu);
      setTimeout(() => {
        document.addEventListener("click", onDocClick, true);
        window.addEventListener("scroll", closeMenu, true);
        window.addEventListener("resize", closeMenu, true);
      }, 0);
    });

    return btn;
  }

  function failPrompt(r: FailRun): string {
    const lines = [
      "Analyze this failed CI run and suggest how to fix it.",
      "",
      `Repo: ${r.full_name} (provider: ${r.provider})`,
      `Branch: ${r.branch}`,
      `Commit: ${r.short_sha}`,
      `Run: ${r.run_label}`,
      `Git id (run URL): ${r.web_url}`,
    ];
    if (r.error && r.error.trim()) {
      lines.push("", "Error / failure summary:", r.error.trim());
    } else {
      lines.push("", "No inline failure log was captured — open the run URL for the full log.");
    }
    lines.push("", "Explain the likely root cause and propose concrete fixes.");
    return lines.join("\n");
  }

  /// Lazily fetch the genuine failure text for a run (the captured summary).
  async function runCopyContent(r: FailRun): Promise<string> {
    try {
      return await invoke<string>(CMD.fetchGitContent, {
        kind: "run",
        accountId: r.account_id,
        fullName: r.full_name,
        number: null,
        cachedError: r.error,
      });
    } catch {
      return failPrompt(r);
    }
  }

  /// Lazily fetch the PR/MR description (+ diff) from the provider API.
  async function prCopyContent(p: OpenPull): Promise<string> {
    try {
      return await invoke<string>(CMD.fetchGitContent, {
        kind: "pr",
        accountId: p.account_id,
        fullName: p.full_name,
        number: p.number,
        cachedError: "",
      });
    } catch {
      return prPrompt(p);
    }
  }

  function prPrompt(p: OpenPull): string {
    return [
      "Review this pull request.",
      "",
      `Repo: ${p.full_name} (provider: ${p.provider})`,
      `PR: #${p.number} ${p.title}`,
      `Author: ${p.author_display}`,
      `Branch: ${p.branch}`,
      `Git id (PR URL): ${p.web_url}`,
      "",
      "Summarize what this PR does, note risks, and suggest improvements.",
    ].join("\n");
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
      if (r.default_branch && r.default_branch_sha) {
        body.append(detailLinePair("branch", r.default_branch, "sha", r.default_branch_sha));
      } else {
        if (r.default_branch) body.append(detailLine("branch", r.default_branch));
        if (r.default_branch_sha) body.append(detailLine("sha", r.default_branch_sha));
      }
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

  function detailLinePair(k1: string, v1: string, k2: string, v2: string): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "gm-dl";
    const a = document.createElement("span");
    a.className = "gm-dl-k";
    a.textContent = k1;
    const b = document.createElement("span");
    b.className = "gm-dl-v";
    b.textContent = v1;
    const sep = document.createElement("span");
    sep.className = "gm-dl-sep";
    sep.textContent = "\u00B7";
    const c = document.createElement("span");
    c.className = "gm-dl-k";
    c.textContent = k2;
    const d = document.createElement("span");
    d.className = "gm-dl-v";
    d.textContent = v2;
    wrap.append(a, b, sep, c, d);
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
