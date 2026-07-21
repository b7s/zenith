import "../../../src/styles/globals.css";
import "./ai-manager.css";
import { mountWindow } from "../../../src/shared/window";
import { setIcon } from "../../../src/shared/icon";
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

  const wrap = document.createElement("div");
  wrap.className = "am-content";
  content.append(wrap);

  // ---- CLI list section ----------------------------------------------------
  const cliSection = document.createElement("section");
  cliSection.className = "am-section";
  const cliTitle = document.createElement("h3");
  cliTitle.className = "am-section-title";
  cliTitle.textContent = "CLIs";
  cliSection.append(cliTitle);
  const cliList = document.createElement("div");
  cliList.className = "am-cli-list";
  cliSection.append(cliList);
  wrap.append(cliSection);

  // ---- toolbar (detect + ack) ---------------------------------------------
  const toolbar = document.createElement("div");
  toolbar.className = "am-toolbar";
  wrap.append(toolbar);

  const installBtn = document.createElement("button");
  installBtn.type = "button";
  installBtn.className = "zen-button is-outline is-sm";
  installBtn.textContent = "Detect & install hooks";
  toolbar.append(installBtn);

  const ackBtn = document.createElement("button");
  ackBtn.type = "button";
  ackBtn.className = "zen-button is-ghost is-sm";
  ackBtn.style.marginLeft = "auto";
  ackBtn.textContent = "Ack failures";
  toolbar.append(ackBtn);

  // ---- failures section ----------------------------------------------------
  const failSection = document.createElement("section");
  failSection.className = "am-section";
  const failTitle = document.createElement("h3");
  failTitle.className = "am-section-title";
  failTitle.textContent = "Unseen failures";
  failSection.append(failTitle);
  const failList = document.createElement("div");
  failSection.append(failList);
  wrap.append(failSection);

  // ---- load initial state --------------------------------------------------
  try {
    state = await invoke<AggregateState>(CMD.getAiCliState);
  } catch (_) { /* default */ }
  try {
    detected = await invoke<CliDetected[]>(CMD.detectAiClis);
  } catch (_) { /* default */ }
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
  } catch (_) { /* default */ }

  render();

  // ---- live updates --------------------------------------------------------
  listen<AggregateState>(EVENT.aiCliChanged, (e) => {
    state = e.payload;
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

      // Status dot
      const dot = document.createElement("span");
      dot.className = "am-cli-status";
      if (enabledClis[info.id]) {
        if (state.has_unseen_failure) dot.classList.add("is-failure");
        else if (state.any_running) dot.classList.add("is-running");
        else dot.classList.add("is-idle");
      }
      row.append(dot);

      // Name
      const nameSpan = document.createElement("span");
      nameSpan.className = "am-cli-name";
      nameSpan.textContent = info.label;
      row.append(nameSpan);

      // Meta (version if found)
      const metaSpan = document.createElement("span");
      metaSpan.className = "am-cli-meta";
      if (found && version) metaSpan.textContent = `v${version}`;
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
      if (!found) {
        const nf = document.createElement("span");
        nf.className = "am-cli-not-found";
        nf.textContent = "Not found";
        row.append(nf);
      }

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
