import "../../../src/styles/globals.css";
import "./ai-cli.css";
import { mountWindow } from "../../../src/shared/window";
import { setIcon } from "../../../src/shared/icon";
import { initLog, logInfo } from "../../../src/shared/log";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CMD } from "../../../src/shared/ipc";
import { EVENT } from "../../../src/shared/events";
import type { AicliState, CliSession, CliStatus } from "../../../src/shared/types";

void (async () => {
  await initLog();
  logInfo("ai-cli window open");

  const { content } = await mountWindow({ title: "AI Agents" });

  const list = document.createElement("div");
  list.className = "ai-list";
  content.append(list);

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
    sub.textContent = subText(rows, agg, installed);
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

  function subText(rows: CliSession[], agg: CliStatus | null, installed: boolean): string {
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
})();
