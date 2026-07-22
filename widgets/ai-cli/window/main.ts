import "../../../src/styles/globals.css";
import "./ai-manager.css";
import { mountWindow } from "../../../src/shared/window";
import { mountTabs } from "../../../src/shared/tabs";
import { initLog, logInfo, logMemory } from "../../../src/shared/log";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CMD } from "../../../src/shared/ipc";
import { EVENT } from "../../../src/shared/events";
import type { AggregateState, CliDetected, Config, CliSnapshot } from "../../../src/shared/types";

void (async () => {
  await initLog();
  logMemory("startup");

  let state: AggregateState = { has_unseen_failure: false, any_running: false, any_finished: false, per_cli: [] };
  let detected: CliDetected[] = [];
  let enabledClis: Record<string, boolean> = { opencode: true, claude: false, codex: false };

  const { content } = await mountWindow({ title: "AI CLI" });

  // ---- tabs (CLIs / Failures) ---------------------------------------------
  const tabs = mountTabs(content, [
    { id: "clis", label: "CLIs" },
    { id: "failures", label: "Failures" },
  ]);
  content.prepend(tabs.container);

  const clisPane = tabs.panes.clis;
  const failuresPane = tabs.panes.failures;

  // Red badge on Failures tab when there are unseen failures
  const failTabBtn = tabs.container.querySelector<HTMLButtonElement>('[data-tab-id="failures"]');
  const failBadge = document.createElement("span");
  failBadge.className = "am-tab-badge";
  failTabBtn?.append(failBadge);

  // ---- CLIs tab: toolbar on top + list below ------------------------------
  clisPane.classList.add("am-pane");

  const toolbar = document.createElement("div");
  toolbar.className = "am-toolbar";
  clisPane.append(toolbar);

  const installBtn = document.createElement("button");
  installBtn.type = "button";
  installBtn.className = "zen-button is-outline is-sm";
  installBtn.style.marginLeft = "auto";
  installBtn.textContent = "Detect & install hooks";
  toolbar.append(installBtn);

  const cliList = document.createElement("div");
  cliList.className = "am-cli-list";
  clisPane.append(cliList);

  // ---- Failures tab: clear-all + list -------------------------------------
  failuresPane.classList.add("am-pane");

  const failToolbar = document.createElement("div");
  failToolbar.className = "am-toolbar";
  failuresPane.append(failToolbar);

  const ackBtn = document.createElement("button");
  ackBtn.type = "button";
  ackBtn.className = "zen-button is-outline is-sm";
  ackBtn.style.marginLeft = "auto";
  ackBtn.textContent = "Clear all";
  failToolbar.append(ackBtn);

  const failList = document.createElement("div");
  failList.className = "am-fail-list";
  failuresPane.append(failList);

  // ---- load initial state --------------------------------------------------
  // Try cache first for instant paint, then refresh in parallel
  const CACHE_KEY = "zenith:ai-cli-manager:state";
  try {
    const cached = localStorage.getItem(CACHE_KEY);
    if (cached) {
      const parsed = JSON.parse(cached) as {
        state: AggregateState;
        detected: CliDetected[];
        enabled: Record<string, boolean>;
      };
      state = parsed.state;
      detected = parsed.detected;
      enabledClis = parsed.enabled;
      render();
    }
  } catch (_) { /* ignore */ }

  // Parallel fetch — 3 sequential invokes were the bottleneck
  const [stateRes, detRes, cfgRes] = await Promise.allSettled([
    invoke<AggregateState>(CMD.getAiCliState),
    invoke<CliDetected[]>(CMD.detectAiClis),
    invoke<Config>(CMD.getConfig),
  ]);
  if (stateRes.status === "fulfilled") state = stateRes.value;
  if (detRes.status === "fulfilled") detected = detRes.value;
  if (cfgRes.status === "fulfilled") {
    const cfg = cfgRes.value;
    const wc = cfg.widgets?.config?.["ai-cli"];
    if (wc && typeof wc === "object") {
      for (const key of ["opencode", "claude", "codex"]) {
        if (typeof (wc as Record<string, unknown>)[key] === "boolean") {
          enabledClis[key] = (wc as Record<string, boolean>)[key];
        }
      }
    }
  }

  // Persist to cache for next open
  try {
    localStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ state, detected: detected.slice(0, 6), enabled: enabledClis }),
    );
  } catch (_) { /* ignore */ }

  render();
  if (state.has_unseen_failure) tabs.switchTo("failures");

  // ---- live updates --------------------------------------------------------
  listen<AggregateState>(EVENT.aiCliChanged, (e) => {
    state = e.payload;
    try {
      localStorage.setItem(
        CACHE_KEY,
        JSON.stringify({ state, detected: detected.slice(0, 6), enabled: enabledClis }),
      );
    } catch (_) { /* ignore */ }
    render();
  });
  listen("zenith:config-updated", async () => {
    try {
      const cfg = await invoke<Config>(CMD.getConfig);
      const wc = cfg.widgets?.config?.["ai-cli"];
      if (wc && typeof wc === "object") {
        for (const key of ["opencode", "claude", "codex"]) {
          if (typeof (wc as Record<string, unknown>)[key] === "boolean") {
            enabledClis[key] = (wc as Record<string, boolean>)[key];
          }
        }
      }
    } catch (_) { /* ignore */ }
    render();
  });

  // ---- button handlers -----------------------------------------------------
  installBtn.addEventListener("click", async () => {
    installBtn.disabled = true;
    try {
      const toEnable = Object.entries(enabledClis)
        .filter(([, v]) => v)
        .map(([k]) => k);
      await invoke(CMD.installAiCliHooks, { clis: toEnable });
      detected = await invoke<CliDetected[]>(CMD.detectAiClis);
      render();
    } catch (e) {
      console.error("[ai-cli-manager] install failed", e);
    }
    installBtn.disabled = false;
  });

  ackBtn.addEventListener("click", async () => {
    await invoke(CMD.ackAiCliFailures);
  });

  logMemory("after mount");
  logInfo("ai-cli-manager ready");

  // ---- render -------------------------------------------------------------
  function render() {
    failBadge.classList.toggle("is-visible", !!state.has_unseen_failure);
    renderCliList();
    renderFailures();
  }

  function renderCliList() {
    cliList.textContent = "";

    const allIds: Array<{ id: string; label: string }> = [
      { id: "opencode", label: "opencode" },
      { id: "claude", label: "Claude Code" },
      { id: "codex", label: "Codex" },
    ];

    for (const info of allIds) {
      const det = detected.find((d) => d.cli_id === info.id);
      const found = det?.installed ?? false;
      const version = det?.version ?? "";

      const row = document.createElement("div");
      row.className = "am-cli-row";
      if (!found) row.classList.add("is-not-found");

      // Status dot — per-CLI from per_cli snapshots
      const dot = document.createElement("span");
      dot.className = "am-cli-status";
      const snap = state.per_cli?.find((s) => s.cli_id === info.id);
      if (snap?.is_waiting) {
        dot.classList.add("is-waiting");
      } else if (snap?.is_running) {
        dot.classList.add("is-running");
      } else if (snap?.last_error_message) {
        dot.classList.add("is-failure");
      }
      row.append(dot);

      // Name
      const nameSpan = document.createElement("span");
      nameSpan.className = "am-cli-name";
      nameSpan.textContent = info.label;
      row.append(nameSpan);

      // Meta — status text takes priority over version
      const metaSpan = document.createElement("span");
      metaSpan.className = "am-cli-meta";
      let statusText = "";
      if (!found) {
        statusText = "not found";
        metaSpan.classList.add("is-status");
      } else if (snap?.is_waiting) {
        statusText = snap.status_text || "waiting confirmation";
        metaSpan.classList.add("is-status");
      } else if (snap?.is_running) {
        statusText = snap.status_text || "running";
        metaSpan.classList.add("is-status");
      } else if (snap?.last_error_message) {
        statusText = snap.status_text || "failed";
        metaSpan.classList.add("is-status");
      } else if (found && version) {
        statusText = `v${version}`;
      }
      metaSpan.textContent = statusText;
      row.append(metaSpan);

      // Toggle switch (settings-style zen-checkbox + zen-checkbox__switch)
      const toggle = document.createElement("label");
      toggle.className = "zen-checkbox";
      const switchEl = document.createElement("span");
      switchEl.className = "zen-checkbox__switch";
      const isEnabled = enabledClis[info.id] ?? false;
      if (!found && isEnabled) {
        enabledClis[info.id] = false;
      }
      const checked = !found ? false : isEnabled;
      if (checked) switchEl.classList.add("is-on");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = checked;
      switchEl.append(cb);
      const track = document.createElement("span");
      track.className = "zen-checkbox__track";
      const thumb = document.createElement("span");
      thumb.className = "zen-checkbox__thumb";
      track.append(thumb);
      switchEl.append(track);
      toggle.append(switchEl);
      if (!found) {
        cb.disabled = true;
      } else {
        cb.addEventListener("change", async () => {
          switchEl.classList.toggle("is-on", cb.checked);
          enabledClis[info.id] = cb.checked;
          try {
            const cfg = await invoke<Config>(CMD.getConfig);
            if (!cfg.widgets.config["ai-cli"]) {
              cfg.widgets.config["ai-cli"] = {} as Record<string, unknown>;
            }
            (cfg.widgets.config["ai-cli"] as Record<string, unknown>)[info.id] = cb.checked;
            await invoke(CMD.saveConfig, { config: cfg });
          } catch (e) {
            console.error("[ai-cli-manager] save config failed", e);
            cb.checked = !cb.checked;
          }
        });
      }
      row.append(toggle);

      cliList.append(row);
    }
  }

  function renderFailures() {
    failList.textContent = "";

    // Derive failures from per_cli snapshots that have an error
    const snapshots: CliSnapshot[] = state.per_cli ?? [];
    const errored = snapshots.filter((s) => s.last_error_message && s.last_error_message.length > 0);

    if (errored.length === 0) {
      const empty = document.createElement("div");
      empty.className = "am-empty";
      const t = document.createElement("div");
      t.className = "am-empty-title";
      t.textContent = "No unseen failures";
      const h = document.createElement("p");
      h.className = "zen-hint";
      h.textContent = "All AI CLI sessions completed without error.";
      empty.append(t, h);
      failList.append(empty);
      return;
    }

    for (const s of errored) {
      const item = document.createElement("div");
      item.className = "am-fail-item";

      const cliSpan = document.createElement("span");
      cliSpan.className = "am-fail-cli";
      cliSpan.textContent = s.cli_id ?? "unknown";
      item.append(cliSpan);

      const msgSpan = document.createElement("span");
      msgSpan.className = "am-fail-msg";
      msgSpan.textContent = s.last_error_message ?? "Unknown error";
      item.append(msgSpan);

      if (s.last_error_at) {
        const timeSpan = document.createElement("span");
        timeSpan.className = "am-fail-time";
        timeSpan.textContent = relAge(s.last_error_at);
        item.append(timeSpan);
      }

      failList.append(item);
    }
  }
})();

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
