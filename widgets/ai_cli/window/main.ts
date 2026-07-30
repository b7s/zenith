import "../../../src/styles/globals.css";
import "./ai-cli.css";
import { mountWindow } from "../../../src/shared/window";
import { setIcon } from "../../../src/shared/icon";
import { initLog, logInfo } from "../../../src/shared/log";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CMD } from "../../../src/shared/ipc";
import { EVENT } from "../../../src/shared/events";
import { mountTabs, type TabMount } from "../../../src/shared/tabs";
import { mountFilterPills } from "../../../src/shared/filter-pills";
import { formatShortDay } from "../../../src/shared/date";
import { axisMax, niceMax, drawYAxis, drawXAxis, drawPeakLine, makeTooltip, attachTooltip, makeHitDot } from "../../../src/shared/chart";
import type {
  AicliState,
  CliSession,
  CliStatus,
  MonthlyUsage,
  DailyUsage,
} from "../../../src/shared/types";

void (async () => {
  await initLog();
  logInfo("ai-cli window open");

  const { content } = await mountWindow({ title: "AI Agents" });

  const tabs: TabMount = mountTabs(
    content,
    [
      { id: "cli", label: "CLI" },
      { id: "usage", label: "Usage" },
    ],
    "cli",
  );
  content.prepend(tabs.container);

  const configLink = document.createElement("button");
  configLink.type = "button";
  configLink.className = "zen-tab zen-tab--action";
  configLink.title = "Configure AI CLI";
  configLink.setAttribute("aria-label", "Configure AI CLI");
  const configIcon = document.createElement("span");
  configIcon.className = "zen-icon";
  setIcon(configIcon, "config", { size: 14 });
  configLink.append(configIcon);
  configLink.addEventListener("click", () =>
    void invoke(CMD.openWidgetConfig, { widgetId: "ai_cli" }),
  );
  tabs.container.append(configLink);

  const list = document.createElement("div");
  list.className = "ai-list";
  tabs.panes["cli"].append(list);

  const usageRoot = document.createElement("div");
  usageRoot.className = "ai-usage";
  tabs.panes["usage"].append(usageRoot);

  const ICONS: Record<string, string> = {
    claude: "terminal-window",
    codex: "terminal-window",
    opencode: "terminal-window",
  };
  const STATUS_LABEL: Record<CliStatus, string> = {
    running: "Running",
    waiting: "Waiting",
    failed: "Failed",
    idle: "Idle",
  };
  /// Stable display order of CLI ids.
  const CLI_ORDER: string[] = ["claude", "codex", "opencode"];

  function relTime(ms: number): string {
    const diff = Date.now() - ms;
    if (diff < 0) return "just now";
    const s = Math.floor(diff / 1000);
    if (s < 60) return s + "s ago";
    const m = Math.floor(s / 60);
    if (m < 60) return m + "m ago";
    const h = Math.floor(m / 60);
    if (h < 24) return h + "h ago";
    const d = Math.floor(h / 24);
    return d + "d ago";
  }

  function isActive(s: CliSession): boolean {
    return s.installed && (s.status === "running" || s.status === "waiting");
  }

  /// Aggregate status across all sessions of a CLI. Worst wins:
  /// failed > waiting > running > idle. `None` only when no sessions exist.
  function aggregateStatus(rows: CliSession[]): CliStatus | null {
    if (rows.length === 0) return null;
    let hasFailed = false,
      hasWaiting = false,
      hasRunning = false;
    for (const s of rows) {
      if (!s.installed) continue;
      if (s.status === "failed") hasFailed = true;
      else if (s.status === "waiting") hasWaiting = true;
      else if (s.status === "running") hasRunning = true;
    }
    if (hasFailed) return "failed";
    if (hasWaiting) return "waiting";
    if (hasRunning) return "running";
    return "idle";
  }

  /// Count active (running + waiting) sessions of a CLI — for the count chip.
  function activeCount(rows: CliSession[]): number {
    return rows.filter((s) => isActive(s) || (s.installed && s.status === "failed")).length;
  }

  /** Render: one `<details>` per CLI, with per-session rows inside the body.
   *  In-place diff so the user's `<details open>` toggle (per CLI) survives
   *  the 3s poll + `aicli-changed` event. */
  function render(state: AicliState): void {
    const sessions = state.sessions || [];

    // Group sessions by CLI id, preserving canonical CLI order.
    const byCli = new Map<string, CliSession[]>();
    for (const s of sessions) {
      if (!byCli.has(s.id)) byCli.set(s.id, []);
      byCli.get(s.id)!.push(s);
    }

    // Include every CLI we know about, even if it has zero sessions
    // (the card still shows up as "Not installed"/"Idle").
    const cliIds: string[] = CLI_ORDER.filter((id) => byCli.has(id));
    for (const id of byCli.keys()) {
      if (!cliIds.includes(id)) cliIds.push(id);
    }

    // Index existing DOM panels by CLI id so we can patch in place.
    const existing = new Map<string, HTMLDetailsElement>();
    for (const el of Array.from(list.children) as HTMLDetailsElement[]) {
      const key = el.dataset.cli;
      if (key) existing.set(key, el);
    }

    const frag = document.createDocumentFragment();
    for (const cliId of cliIds) {
      const rows = byCli.get(cliId) || [];
      const sig = cliSignature(rows);
      const cur = existing.get(cliId);

      let panel: HTMLDetailsElement;
      if (cur && cur.dataset.sig === sig) {
        // Identical: reuse without touching (preserves open + DOM).
        panel = cur;
      } else if (cur) {
        // Same CLI, content changed: patch in place, preserve open.
        patchPanel(cur, cliId, rows);
        panel = cur;
      } else {
        panel = buildPanel(cliId, rows);
      }
      frag.append(panel);
    }

    list.replaceChildren(frag);
  }

  /** Byte-for-byte signature of the CLI's visible state. Twins match → no
   *  DOM mutation → `<details open>` survives user clicks. */
  function cliSignature(rows: CliSession[]): string {
    const parts: string[] = [String(rows.length)];
    for (const s of rows) {
      parts.push(
        [
          s.installed ? 1 : 0,
          s.status,
          s.running ? 1 : 0,
          s.title || "",
          s.cwd || "",
          s.updated_ms,
        ].join(":"),
      );
    }
    return parts.join("|");
  }

  function buildPanel(cliId: string, rows: CliSession[]): HTMLDetailsElement {
    const panel = document.createElement("details");
    panel.className = "ai-panel";
    panel.dataset.cli = cliId;
    panel.dataset.sig = cliSignature(rows);

    const agg = aggregateStatus(rows);
    const installed = rows.some((s) => s.installed);
    const failed = agg === "failed";
    if ((agg === "running" || agg === "waiting" || failed) && rows.length > 0) {
      panel.open = true;
    }
    if (failed) panel.classList.add("is-alert");

    panel.append(summaryDom(cliId, rows, agg, installed), bodyDom(cliId, rows, installed));

    // Sync chevron icon with open/closed state on every toggle.
    panel.addEventListener("toggle", () => {
      const chev = panel.querySelector<HTMLElement>(".ai-panel__chevron");
      if (chev) setIcon(chev, panel.open ? "caret-up" : "caret-down", { size: 14 });
    });
    // Correct initial icon if auto-opened.
    const initChev = panel.querySelector<HTMLElement>(".ai-panel__chevron");
    if (initChev && panel.open) setIcon(initChev, "caret-up", { size: 14 });

    return panel;
  }

  function patchPanel(panel: HTMLDetailsElement, cliId: string, rows: CliSession[]): void {
    panel.dataset.sig = cliSignature(rows);

    const agg = aggregateStatus(rows);
    const installed = rows.some((s) => s.installed);
    const failed = agg === "failed";
    const wasOpen = panel.open;

    // Re-evaluate the auto-open rule only when NOT manually toggled. We
    // can't perfectly distinguish "user clicked" from "auto-opened"; we
    // keep the current state if it would no longer auto-open.
    const shouldAuto = (agg === "running" || agg === "waiting" || failed) && rows.length > 0;
    if (!wasOpen && shouldAuto) {
      panel.open = true;
    } else {
      panel.open = wasOpen;
    }

    panel.classList.toggle("is-alert", failed);

    // Replace summary + body in place; preserves the `<details>` toggle.
    panel.replaceChildren(summaryDom(cliId, rows, agg, installed), bodyDom(cliId, rows, installed));
  }

  function summaryDom(
    cliId: string,
    rows: CliSession[],
    agg: CliStatus | null,
    installed: boolean,
  ): HTMLElement {
    const summary = document.createElement("summary");

    const icon = document.createElement("span");
    icon.className = "ai-panel__icon";
    setIcon(icon, ICONS[cliId] || "terminal-window", { size: 18 });

    const label = document.createElement("div");
    label.className = "ai-panel__label";
    const name = document.createElement("span");
    name.className = "ai-panel__name";
    name.textContent = cliLabel(cliId);
    const sub = document.createElement("span");
    sub.className = "ai-panel__sub";
    sub.textContent = subText(rows, installed);
    label.append(name, sub);

    // Plus button — opens a new terminal with the CLI running inside it.
    const plus = document.createElement("button");
    plus.className = "ai-panel__plus";
    plus.title = "New " + cliLabel(cliId) + " session";
    setIcon(plus, "plus", { size: 14 });
    plus.addEventListener("click", (e) => {
      e.stopPropagation();
      void invoke(CMD.startCli, { id: cliId });
    });

    const status = document.createElement("span");
    status.className = "ai-panel__status";
    const st: CliStatus = agg ?? "idle";
    status.dataset.status = st;
    const dot = document.createElement("span");
    dot.className = "ai-panel__dot";
    const txt = document.createElement("span");
    txt.textContent = !installed ? "Not installed" : STATUS_LABEL[st] || "Idle";
    status.append(dot, txt);

    // Count chip — only renders when > 0 active sessions.
    let countChip: HTMLElement | null = null;
    const active = activeCount(rows);
    if (active > 0) {
      countChip = document.createElement("span");
      countChip.className = "ai-panel__count";
      countChip.textContent = String(active);
    }

    const chev = document.createElement("span");
    chev.className = "ai-panel__chevron";
    setIcon(chev, "caret-down", { size: 14 });

    if (countChip) {
      summary.append(icon, label, plus, status, countChip, chev);
    } else {
      summary.append(icon, label, plus, status, chev);
    }
    return summary;
  }

  function subText(rows: CliSession[], installed: boolean): string {
    if (!installed) return "Not installed";
    if (rows.length === 0) return "—";
    const running = rows.filter((s) => s.installed && s.status === "running").length;
    const waiting = rows.filter((s) => s.installed && s.status === "waiting").length;
    const failed = rows.filter((s) => s.installed && s.status === "failed").length;
    const parts: string[] = [];
    if (running) parts.push(running + " running");
    if (waiting) parts.push(waiting + " waiting");
    if (failed) parts.push(failed + " failed");
    if (parts.length === 0) return rows.length + " session" + (rows.length > 1 ? "s" : "") + " · idle";
    return parts.join(" · ");
  }

  function bodyDom(cliId: string, rows: CliSession[], installed: boolean): HTMLElement {
    const body = document.createElement("div");
    body.className = "ai-panel__body";

    if (!installed) {
      const empty = document.createElement("div");
      empty.className = "ai-row__empty";
      empty.textContent = "Install " + cliLabel(cliId) + " to monitor its sessions.";
      body.append(empty);
      return body;
    }

    if (rows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "ai-row__empty";
      empty.textContent = "No sessions detected.";
      body.append(empty);
      return body;
    }

    // Show running/waiting/failed first, then idle — most relevant on top.
    const ORDER: CliStatus[] = ["running", "waiting", "failed", "idle"];
    const sorted = [...rows].sort((a, b) => {
      const ai = ORDER.indexOf(a.status);
      const bi = ORDER.indexOf(b.status);
      if (ai !== bi) return ai - bi;
      return (b.updated_ms || 0) - (a.updated_ms || 0);
    });

    for (const s of sorted) {
      body.append(rowDom(s));
    }
    return body;
  }

  function rowDom(s: CliSession): HTMLElement {
    const row = document.createElement("div");
    row.className = "ai-row";
    row.dataset.status = s.installed ? (s.running ? s.status : "idle") : "idle";

    const bar = document.createElement("span");
    bar.className = "ai-row__bar";

    const main = document.createElement("div");
    main.className = "ai-row__main";
    const title = document.createElement("span");
    title.className = "ai-row__title";
    title.textContent = s.title || s.cwd || (s.installed ? "Session" : "Not installed");
    const path = document.createElement("span");
    path.className = "ai-row__path";
    path.textContent = s.cwd || "—";
    main.append(title, path);

    const chip = document.createElement("span");
    chip.className = "ai-row__chip";
    chip.dataset.state = s.installed ? (s.running ? s.status : "idle") : "idle";
    chip.textContent = chipLabel(s, s.installed ? (s.running ? s.status : "idle") : "idle");

    row.append(bar, main, chip);
    return row;
  }

  function chipLabel(s: CliSession, st: CliStatus): string {
    if (!s.installed) return "—";
    let label = STATUS_LABEL[st] || "Idle";
    if (s.updated_ms > 0) {
      label += " · " + relTime(s.updated_ms);
    }
    return label;
  }

  function cliLabel(cli: string): string {
    if (cli === "claude") return "Claude Code";
    if (cli === "codex") return "Codex";
    if (cli === "opencode") return "OpenCode";
    return cli.charAt(0).toUpperCase() + cli.slice(1);
  }

  try {
    const state = await invoke<AicliState>(CMD.getAicliState);
    render(state);
  } catch {
    // poll timer will populate
  }

  let timer: number | null = null;
  async function refresh(): Promise<void> {
    try {
      const state = await invoke<AicliState>(CMD.getAicliState);
      render(state);
    } catch {
      /* ignore */
    }
  }

  timer = window.setInterval(() => void refresh(), 3000);

  const unlisten = await listen<AicliState>(EVENT.aicliChanged, (e) => {
    if (e && e.payload) render(e.payload);
  }).catch(() => null);

  window.addEventListener("beforeunload", () => {
    if (timer !== null) clearInterval(timer);
    if (unlisten) unlisten();
  });

  // ── Usage tab ──────────────────────────────────────────────────────
  let usageData: MonthlyUsage | null = null;
  let usageFilter = "all";
  let usageLoaded = false;
  let usageMonth = currentMonth();

  function currentMonth(): string {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
  }

  function lastMonths(count: number): string[] {
    const [y, m] = usageMonth.split("-").map(Number);
    const out: string[] = [];
    for (let i = 0; i < count; i++) {
      let month = m - i;
      let year = y;
      while (month < 1) { month += 12; year--; }
      out.push(`${year}-${String(month).padStart(2, "0")}`);
    }
    return out;
  }

  tabs.container.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLElement>("[data-tab-id]");
    if (btn && btn.dataset.tabId === "usage" && !usageLoaded) {
      usageLoaded = true;
      void loadUsage();
    }
  });

  async function loadUsage(): Promise<void> {
    try {
      usageData = await invoke<MonthlyUsage>(CMD.getMonthlyUsage, { month: usageMonth });
      renderUsage();
    } catch {
      usageRoot.textContent = "Failed to load usage data.";
    }
  }

  function formatTokens(n: number): string {
    if (n >= 1e15) return (n / 1e15).toFixed(2) + "Q";
    if (n >= 1e12) return (n / 1e12).toFixed(2) + "T";
    if (n >= 1e9) return (n / 1e9).toFixed(1) + "B";
    if (n >= 1e6) return (n / 1e6).toFixed(1) + "M";
    if (n >= 1e3) return (n / 1e3).toFixed(0) + "K";
    return String(n);
  }

  function formatCost(n: number): string {
    return "$" + n.toFixed(2);
  }

  // ── Chart helpers imported from src/shared/chart.ts ──
  // niceMax, axisMax, drawYAxis, drawXAxis, drawPeakLine, makeTooltip, attachTooltip, makeHitDot

  function metricCard(label: string, value: string, cls: string): HTMLElement {
    const card = document.createElement("div");
    card.className = "zen-card ai-usage__metric " + cls;
    const val = document.createElement("span");
    val.className = "ai-usage__metric-value";
    val.textContent = value;
    const lab = document.createElement("span");
    lab.className = "ai-usage__metric-label";
    lab.textContent = label;
    card.append(val, lab);
    return card;
  }

  function renderUsage(): void {
    usageRoot.innerHTML = "";

    const scroll = document.createElement("div");
    scroll.className = "ai-usage__scroll";
    usageRoot.append(scroll);

    const pillsW = document.createElement("div");
    pillsW.className = "ai-usage__pills";
    const pillsWrap = document.createElement("div");
    pillsWrap.className = "ai-usage__pills-left";
    const pills = mountFilterPills(pillsWrap, [
      { id: "all", label: "All" },
      { id: "claude", label: "Claude Code" },
      { id: "codex", label: "Codex" },
      { id: "opencode", label: "OpenCode" },
    ], "all");
    pillsW.append(pillsWrap);

    const monthSel = document.createElement("select");
    monthSel.className = "ai-usage__month-select";
    const months = lastMonths(3);
    for (const m of months) {
      const opt = document.createElement("option");
      opt.value = m;
      opt.textContent = m;
      if (m === usageMonth) opt.selected = true;
      monthSel.append(opt);
    }
    monthSel.addEventListener("change", async () => {
      usageMonth = monthSel.value;
      usageData = await invoke<MonthlyUsage>(CMD.getMonthlyUsage, { month: usageMonth });
      renderUsageContent();
    });
    pillsW.append(monthSel);

    const updateBtn = document.createElement("button");
    updateBtn.className = "ai-usage__update-btn";
    updateBtn.title = "Refresh usage data";
    const updateIcon = document.createElement("span");
    updateIcon.className = "zen-icon";
    setIcon(updateIcon, "arrows-clockwise", { size: 16 });
    updateBtn.append(updateIcon);
    updateBtn.addEventListener("click", async () => {
      updateBtn.classList.add("is-loading");
      try {
        usageData = await invoke<MonthlyUsage>(CMD.getMonthlyUsage, { month: usageMonth });
        renderUsageContent();
      } catch {
        // ignore
      } finally {
        updateBtn.classList.remove("is-loading");
      }
    });
    pillsW.append(updateBtn);
    scroll.append(pillsW);

    pills.container.addEventListener("click", (e: PointerEvent) => {
      const btn = (e.target as HTMLElement).closest<HTMLElement>("[data-pill-id]");
      if (!btn) return;
      const next = btn.dataset.pillId;
      if (!next || next === usageFilter) return;
      usageFilter = next;
      renderUsageContent();
    });

    function aggregateByDay(data: DailyUsage[]): DailyUsage[] {
      const map = new Map<string, DailyUsage>();
      for (const d of data) {
        const key = d.day;
        const existing = map.get(key);
        if (existing) {
          existing.sessions += d.sessions;
          existing.tokens_input += d.tokens_input;
          existing.tokens_output += d.tokens_output;
          existing.tokens_cache_read += d.tokens_cache_read;
          existing.tokens_cache_write += d.tokens_cache_write;
          existing.cost_usd += d.cost_usd;
        } else {
          map.set(key, { ...d });
        }
      }
      return Array.from(map.values()).sort((a, b) => a.day.localeCompare(b.day));
    }

    function renderUsageContent(): void {
      const existing = scroll.querySelector(".ai-usage__content");
      if (existing) existing.remove();

      const ct = document.createElement("div");
      ct.className = "ai-usage__content";
      scroll.append(ct);
      if (!usageData) return;
      const du = usageData;

      const filtered = usageFilter === "all"
        ? du.daily : du.daily.filter((d) => d.cli_id === usageFilter);

      const totalTokens = filtered.reduce((s, d) => s + d.tokens_input + d.tokens_output + d.tokens_cache_read, 0);
      const dayCount = new Set(filtered.map((d) => d.day)).size;
      const avgDay = dayCount > 0 ? Math.round(totalTokens / dayCount) : 0;
      const totalCost = filtered.reduce((s, d) => s + d.cost_usd, 0);

      // Metric cards
      const mRow = document.createElement("div");
      mRow.className = "ai-usage__metrics";
      mRow.append(
        metricCard("Total Tokens", formatTokens(totalTokens), "ai-usage__metric--tokens"),
        metricCard("Avg / Day", formatTokens(avgDay), "ai-usage__metric--avg"),
        metricCard("Cost", formatCost(totalCost), "ai-usage__metric--cost"),
      );
      ct.append(mRow);

      if (filtered.length === 0) {
        const em = document.createElement("div");
        em.className = "ai-usage__empty";
        em.textContent = "No usage data for this period.";
        ct.append(em);
        return;
      }

      // Aggregate by day for charts (same day can have multiple model entries)
      const chartData = aggregateByDay(filtered);

      // Tokens bar chart
      ct.append(buildTokensChart(chartData, usageFilter));
      // Cost chart
      ct.append(buildCostChart(chartData, usageFilter));

      // Model usage table (uses per-model data, not aggregated)
      ct.append(buildModelTable(filtered));
    }

    function buildModelTable(daily: DailyUsage[]): HTMLElement {
      const CLI_COLOR: Record<string, string> = { claude: "var(--primary)", codex: "#10b981", opencode: "#8b5cf6" };
      type SortKey = "provider" | "model" | "tokens" | "cost";
      // Parse "[provider] model" format and store separately
      const acc = new Map<string, { provider: string; model: string; cli_id: string; tokens: number; cost: number }>();
      for (const d of daily) {
        const a = acc.get(d.model_name) ?? (() => {
          const m = d.model_name.match(/^\[(.+?)\]\s+(.*)$/);
          const provider = m?.[1];
          return {
            provider: provider ?? d.cli_id,
            model: m?.[2] ?? d.model_name,
            cli_id: d.cli_id,
            tokens: 0, cost: 0,
          };
        })();
        a.tokens += d.tokens_input + d.tokens_output + d.tokens_cache_read;
        a.cost += d.cost_usd;
        acc.set(d.model_name, a);
      }
      const entries = Array.from(acc.entries());
      let sortKey: SortKey = "cost";
      let sortAsc = false;

      const wrap = document.createElement("div");
      wrap.className = "zen-card ai-usage__model-table";
      const ttl = document.createElement("div");
      ttl.className = "ai-usage__chart-title";
      ttl.textContent = "Top Models";
      wrap.append(ttl);

      const table = document.createElement("table");
      table.className = "ai-usage__table";
      const thead = document.createElement("thead");
      const tbody = document.createElement("tbody");
      table.append(thead, tbody);

      const cols: { label: string; key: SortKey; num: boolean }[] = [
        { label: "Provider", key: "provider", num: false },
        { label: "Model", key: "model", num: false },
        { label: "Tokens", key: "tokens", num: true },
        { label: "Value", key: "cost", num: true },
      ];

      function render() {
        tbody.innerHTML = "";
        const cmp = sortAsc ? 1 : -1;
        const sorted = [...entries]
          .sort((a, b) => {
            const va = a[1], vb = b[1];
            if (sortKey === "provider") return va.provider.localeCompare(vb.provider) * cmp;
            if (sortKey === "model") return va.model.localeCompare(vb.model) * cmp;
            if (sortKey === "tokens") return (va.tokens - vb.tokens) * cmp;
            return (va.cost - vb.cost) * cmp;
          })
          .slice(0, 5);

        for (const [, v] of sorted) {
          const row = document.createElement("tr");
          const dot = document.createElement("span");
          dot.className = "ai-usage__table-dot";
          dot.style.background = CLI_COLOR[v.cli_id] ?? "var(--muted)";

          const provCell = document.createElement("td");
          const lbl = document.createElement("span");
          lbl.className = "ai-usage__model-provider";
          lbl.textContent = v.provider;
          provCell.append(dot, lbl);

          const modelCell = document.createElement("td");
          modelCell.textContent = v.model;

          const tokCell = document.createElement("td");
          tokCell.className = "is-num";
          tokCell.textContent = formatTokens(v.tokens);

          const costCell = document.createElement("td");
          costCell.className = "is-num";
          costCell.textContent = formatCost(v.cost);

          row.append(provCell, modelCell, tokCell, costCell);
          tbody.append(row);
        }
      }

      const tr = document.createElement("tr");
      for (const c of cols) {
        const th = document.createElement("th");
        if (c.num) th.classList.add("is-num");
        th.classList.add("is-sortable");
        th.dataset.sortKey = c.key;
        tr.append(th);
      }
      thead.append(tr);

      thead.addEventListener("click", (e) => {
        const th = (e.target as HTMLElement).closest("th.is-sortable");
        if (!th) return;
        const key = (th as HTMLElement).dataset.sortKey as SortKey;
        if (sortKey === key) {
          sortAsc = !sortAsc;
        } else {
          sortKey = key;
          sortAsc = key === "model" || key === "provider";
        }
        for (const c of cols) {
          const el = thead.querySelector<HTMLElement>(`[data-sort-key="${c.key}"]`);
          if (!el) continue;
          el.textContent = c.label + (sortKey === c.key ? " " + (sortAsc ? "▲" : "▼") : "");
        }
        render();
      });

      // Trigger initial render of header arrows
      for (const c of cols) {
        const el = thead.querySelector<HTMLElement>(`[data-sort-key="${c.key}"]`);
        if (!el) continue;
        el.textContent = c.label + (sortKey === c.key ? " " + (sortAsc ? "▲" : "▼") : "");
      }
      render();
      wrap.append(table);
      return wrap;
    }

    renderUsageContent();
  }

  function buildTokensChart(daily: DailyUsage[], _filter: string): HTMLElement {
    const W = 360, H = 192, PAD_L = 48, PAD_R = 8, PAD_T = 12, PAD_B = 36;
    const plotW = W - PAD_L - PAD_R, plotH = H - PAD_T - PAD_B;
    const rawMax = Math.max(...daily.map((d) => d.tokens_input + d.tokens_output + d.tokens_cache_read), 1);
    const yMax = axisMax(rawMax);
    const yScale = (v: number): number => PAD_T + plotH - (v / yMax) * plotH;
    const barGap = 2;
    const barCount = daily.length;
    const barW = Math.max(3, (plotW - barGap * (barCount - 1)) / barCount);

    const wrap = document.createElement("div");
    wrap.className = "zen-card ai-usage__chart";
    const ttl = document.createElement("div");
    ttl.className = "ai-usage__chart-title";
    ttl.textContent = "Tokens by Day";
    wrap.append(ttl);

    const svgEl = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svgEl.classList.add("zen-chart__chart-svg");
    svgEl.setAttribute("viewBox", `0 0 ${W} ${H}`);
    svgEl.setAttribute("preserveAspectRatio", "xMidYMid meet");
    svgEl.setAttribute("role", "img");
    svgEl.setAttribute("aria-label", "Tokens by day bar chart");

    const tooltip = makeTooltip(wrap);

    // Axes (drawn first so bars sit on top)
    drawYAxis(svgEl, yMax, formatTokens, PAD_L, PAD_T, plotW, plotH);
    drawXAxis(svgEl, daily.map(d => d.day), formatShortDay, PAD_L, PAD_T, plotW, plotH);

    // Peak dashed line
    drawPeakLine(svgEl, rawMax, yScale, `peak ${formatTokens(rawMax)}`, PAD_L, plotW);

    const xStep = barCount > 1 ? plotW / barCount : 0;
    const xStart = barCount > 1 ? 0 : plotW / 2 - barW / 2;
    for (let i = 0; i < barCount; i++) {
      const d = daily[i];
      const total = d.tokens_input + d.tokens_output + d.tokens_cache_read;
      const barH = (total / yMax) * plotH;
      const x = PAD_L + xStart + i * xStep;
      const y = PAD_T + plotH - barH;

      const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      rect.setAttribute("x", String(x));
      rect.setAttribute("y", String(y));
      rect.setAttribute("width", String(barW));
      rect.setAttribute("height", String(Math.max(1, barH)));
      rect.classList.add("ai-usage__bar");
      svgEl.append(rect);

      const hitCol = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      hitCol.setAttribute("x", String(x - barGap / 2));
      hitCol.setAttribute("y", String(PAD_T));
      hitCol.setAttribute("width", String(barW + barGap));
      hitCol.setAttribute("height", String(plotH));
      hitCol.classList.add("zen-chart__hit");
      attachTooltip(hitCol, tooltip,
        `<strong>${formatShortDay(d.day)}</strong><br>` +
        `Input: ${formatTokens(d.tokens_input)}<br>` +
        `Output: ${formatTokens(d.tokens_output)}<br>` +
        `Cache: ${formatTokens(d.tokens_cache_read)}<br>` +
        `Total: ${formatTokens(total)}`);
      svgEl.append(hitCol);
    }

    wrap.append(svgEl);
    return wrap;
  }

  function buildCostChart(daily: DailyUsage[], _filter: string): HTMLElement {
    const W = 360, H = 192, PAD_L = 48, PAD_R = 8, PAD_T = 12, PAD_B = 36;
    const plotW = W - PAD_L - PAD_R, plotH = H - PAD_T - PAD_B;

    const wrap = document.createElement("div");
    wrap.className = "zen-card ai-usage__chart";
    const ttl = document.createElement("div");
    ttl.className = "ai-usage__chart-title";
    ttl.textContent = "Daily Cost (USD)";
    wrap.append(ttl);

    const svgEl = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svgEl.classList.add("zen-chart__chart-svg");
    svgEl.setAttribute("viewBox", `0 0 ${W} ${H}`);
    svgEl.setAttribute("preserveAspectRatio", "xMidYMid meet");
    svgEl.setAttribute("role", "img");
    svgEl.setAttribute("aria-label", "Daily cost line chart");

    if (daily.length === 0) {
      wrap.append(svgEl);
      return wrap;
    }

    const rawMax = Math.max(...daily.map((d) => d.cost_usd), 0.01);
    const yMax = niceMax(rawMax);

    const tooltip = makeTooltip(wrap);

    // Axes
    drawYAxis(svgEl, yMax, formatCost, PAD_L, PAD_T, plotW, plotH);
    drawXAxis(svgEl, daily.map(d => d.day), formatShortDay, PAD_L, PAD_T, plotW, plotH);

    if (daily.length < 2) {
      // Single point: draw a dot at center
      const cx = PAD_L + plotW / 2;
      const cy = PAD_T + plotH - (daily[0].cost_usd / yMax) * plotH;
      const c = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      c.setAttribute("cx", String(cx));
      c.setAttribute("cy", String(cy));
      c.setAttribute("r", "4");
      c.classList.add("ai-usage__cost-pt");
      svgEl.append(c);
      const hit = makeHitDot(cx, cy);
      attachTooltip(hit, tooltip, `<strong>${formatShortDay(daily[0].day)}</strong><br>${formatCost(daily[0].cost_usd)}`);
      svgEl.append(hit);
      wrap.append(svgEl);
      return wrap;
    }

    const xStep = plotW / (daily.length - 1);
    const yScale = (v: number) => PAD_T + plotH - (v / yMax) * plotH;

    // Gradient fill def
    const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
    const grad = document.createElementNS("http://www.w3.org/2000/svg", "linearGradient");
    grad.id = "ai-cost-grad-" + Math.random().toString(36).slice(2, 6);
    grad.setAttribute("x1", "0"); grad.setAttribute("y1", "0");
    grad.setAttribute("x2", "0"); grad.setAttribute("y2", "1");
    const st1 = document.createElementNS("http://www.w3.org/2000/svg", "stop");
    st1.setAttribute("offset", "0%");
    st1.setAttribute("stop-color", "var(--primary)");
    st1.setAttribute("stop-opacity", "0.25");
    const st2 = document.createElementNS("http://www.w3.org/2000/svg", "stop");
    st2.setAttribute("offset", "100%");
    st2.setAttribute("stop-color", "var(--primary)");
    st2.setAttribute("stop-opacity", "0");
    grad.append(st1, st2);
    defs.append(grad);
    svgEl.append(defs);

    const pts: string[] = [];
    for (let i = 0; i < daily.length; i++) {
      pts.push(`${(PAD_L + i * xStep).toFixed(1)} ${yScale(daily[i].cost_usd).toFixed(1)}`);
    }

    // Area fill
    const areaD = "M " + pts.join(" L ") + " L " + (PAD_L + (daily.length - 1) * xStep).toFixed(1) + " " + yScale(0).toFixed(1) + " L " + PAD_L.toFixed(1) + " " + yScale(0).toFixed(1) + " Z";
    const area = document.createElementNS("http://www.w3.org/2000/svg", "path");
    area.setAttribute("d", areaD);
    area.setAttribute("fill", `url(#${grad.id})`);
    svgEl.append(area);

    // Line
    const line = document.createElementNS("http://www.w3.org/2000/svg", "path");
    line.setAttribute("d", "M " + pts.join(" L "));
    line.classList.add("ai-usage__cost-line");
    svgEl.append(line);

    // Point markers (every point; each has a large hit circle for tooltip)
    for (let i = 0; i < daily.length; i++) {
      const cx = PAD_L + i * xStep;
      const cy = yScale(daily[i].cost_usd);
      const c = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      c.setAttribute("cx", String(cx));
      c.setAttribute("cy", String(cy));
      c.setAttribute("r", "4");
      c.classList.add("ai-usage__cost-pt");
      svgEl.append(c);
      const hit = makeHitDot(cx, cy);
      attachTooltip(hit, tooltip, `<strong>${formatShortDay(daily[i].day)}</strong><br>${formatCost(daily[i].cost_usd)}`);
      svgEl.append(hit);
    }

    wrap.append(svgEl);
    return wrap;
  }
})();
