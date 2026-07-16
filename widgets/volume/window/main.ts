import "../../../src/styles/globals.css";
import "./volume-popup.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { setIcon } from "../../../src/shared/icon";
import { CMD } from "../../../src/shared/ipc";
import { applyTheme } from "../../../src/shared/window";
import { initLog, logInfo } from "../../../src/shared/log";
import type { AppSessionInfo } from "../../../src/shared/types";

// Window geometry contract — mirrors the Rust `inner_size` / `max_inner_size`
// constants in `create_volume_popup_window`. The collapsed popup keeps the
// master row only; opening the mixer accordion grows the window up to
// `MAX_POPUP_H` (400 px, the OS-pixel max enforced by Rust).
const POPUP_W = 300;
const COLLAPSED_H = 72;
const MAX_POPUP_H = 400;

void (async () => {
  await initLog();

  applyTheme();

  const root = document.getElementById("root");
  if (!root) return;

  const win = getCurrentWindow();

  // ----- Chrome skeleton. We don't use `mountWindow` here — the popup is a
  // compact control strip, not a chrome'd window, and the legacy main row is
  // what users already know. The splitter accordion is added below it.

  const wrapper = document.createElement("div");
  wrapper.className = "vol-popup";

  // -- Master row --
  const masterRow = document.createElement("div");
  masterRow.className = "vol-master";

  const masterIconEl = document.createElement("span");
  masterIconEl.className = "vol-master__icon";
  setIcon(masterIconEl, "volume-2", { size: 14 });

  const masterSlider = document.createElement("input");
  masterSlider.type = "range";
  masterSlider.min = "0";
  masterSlider.max = "100";
  masterSlider.value = "50";
  masterSlider.className = "zen-slider vol-master__slider";
  masterSlider.setAttribute("aria-label", "Master volume");

  const masterLabel = document.createElement("span");
  masterLabel.className = "vol-master__label";
  masterLabel.textContent = "50%";

  masterRow.append(masterIconEl, masterSlider, masterLabel);

  // -- Accordion mixer --
  // Uses the shared `.zen-collapse` primitive from components.css. The
  // `<details>` body is built imperatively below; only the body element
  // is rebuilt on each session refresh — the `<details>` element itself
  // is stable so its open/closed state survives refreshes.
  const mixer = document.createElement("details");
  mixer.className = "zen-collapse vol-mixer";

  const mixerSummary = document.createElement("summary");
  const mixerLabel = document.createElement("span");
  mixerLabel.className = "vol-mixer__label";
  const mixerHint = document.createElement("span");
  mixerHint.className = "vol-mixer__hint";
  mixerHint.textContent = "App volumes";
  const mixerCount = document.createElement("span");
  mixerCount.className = "vol-mixer__count";

  const mixerChevron = document.createElement("span");
  mixerChevron.className = "vol-mixer__chevron";
  setIcon(mixerChevron, "chevron-right", { size: 12 });

  // Clickable icon (side-by-side with "App volumes") that opens the native
  // Windows 11 audio settings (the per-app volume mixer).
  const mixerConfig = document.createElement("span");
  mixerConfig.className = "vol-mixer__config";
  setIcon(mixerConfig, "sliders-horizontal", { size: 13 });
  mixerConfig.title = "Open Windows sound settings";
  mixerConfig.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    void invoke(CMD.openUrl, { url: "ms-settings:apps-volume" }).catch(() => {});
  });

  mixerLabel.append(mixerConfig, mixerHint, mixerCount);
  mixerSummary.append(mixerLabel, mixerChevron);
  mixer.append(mixerSummary);

  const mixerBody = document.createElement("div");
  mixerBody.className = "vol-mixer__body";
  mixer.append(mixerBody);

  wrapper.append(masterRow, mixer);
  root.replaceChildren(wrapper);

  // ----- Master volume controls (unchanged from the legacy popup) -----
  function updateMasterIcon(level: number, muted: boolean) {
    let name: string;
    if (muted || level < 0.01) name = "volume-x";
    else if (level < 0.5) name = "volume-1";
    else name = "volume-2";
    setIcon(masterIconEl, name, { size: 14 });
  }
  function updateMasterLabel(level: number) {
    masterLabel.textContent = Math.round(level * 100) + "%";
  }

  try {
    const info = await invoke<{ level: number; muted: boolean }>("get_volume");
    masterSlider.value = String(Math.round(info.level * 100));
    updateMasterIcon(info.level, info.muted);
    updateMasterLabel(info.level);
  } catch {
    logInfo("failed to get initial volume");
  }

  let masterDragging = false;
  masterSlider.addEventListener("pointerdown", () => { masterDragging = true; });
  masterSlider.addEventListener("pointerup", () => { masterDragging = false; });
  masterSlider.addEventListener("pointerleave", () => { masterDragging = false; });
  masterSlider.addEventListener("input", () => {
    const level = Number(masterSlider.value) / 100;
    updateMasterIcon(level, false);
    updateMasterLabel(level);
    invoke("set_volume", { level }).catch(() => {});
  });
  masterSlider.addEventListener("wheel", (e) => {
    e.preventDefault();
    const cur = Number(masterSlider.value);
    const next = Math.max(0, Math.min(100, cur + (e.deltaY > 0 ? -5 : 5)));
    masterSlider.value = String(next);
    const level = next / 100;
    updateMasterIcon(level, false);
    updateMasterLabel(level);
    invoke("set_volume", { level }).catch(() => {});
  }, { passive: false });

  // Click the master icon to toggle mute (mirrors the bar widget's
  // right-click behaviour in a discoverable way).
  let masterMuted = false;
  masterIconEl.addEventListener("click", () => {
    masterMuted = !masterMuted;
    invoke("set_muted", { muted: masterMuted }).catch(() => {});
    updateMasterIcon(Number(masterSlider.value) / 100, masterMuted);
  });

  // ----- Per-app mixer -----
  // Re-pulls the session list on a timer and when the accordion opens. Each
  // refresh rebuilds `mixerBody` from scratch; the `<details>` element is
  // stable, so its open state is preserved and the user never loses a
  // freshly-expanded accordion to a refresh.
  let sessions: AppSessionInfo[] = [];
  // Slider values currently being dragged by the user, keyed by session id.
  // Used to suppress external refreshes from snapping the thumb mid-drag.
  const dragging = new Set<string>();

  function appIcon(level: number, muted: boolean): string {
    if (muted || level < 0.01) return "volume-x";
    if (level < 0.5) return "volume-1";
    return "volume-2";
  }

  function renderMixer() {
    const n = sessions.length;
    mixerCount.textContent = n > 0 ? `${n}` : "";

    if (n === 0) {
      const empty = document.createElement("div");
      empty.className = "vol-mixer__empty";
      empty.textContent = "No apps playing audio right now.";
      mixerBody.replaceChildren(empty);
      return;
    }

    const rows: HTMLElement[] = [];
    for (const s of sessions) {
      const row = document.createElement("div");
      row.className = "vol-app" + (s.muted ? " is-muted" : "");

      const name = document.createElement("span");
      name.className = "vol-app__name";
      name.textContent = s.name;
      name.title = s.name;

      const slider = document.createElement("input");
      slider.type = "range";
      slider.min = "0";
      slider.max = "100";
      slider.value = String(Math.round(s.level * 100));
      slider.className = "zen-slider vol-app__slider";
      slider.setAttribute("aria-label", `${s.name} volume`);

      const pct = document.createElement("span");
      pct.className = "vol-app__pct";
      pct.textContent = Math.round(s.level * 100) + "%";

      const muteBtn = document.createElement("button");
      muteBtn.type = "button";
      muteBtn.className = "zen-icon-button vol-app__mute";
      muteBtn.setAttribute("aria-label", s.muted ? `Unmute ${s.name}` : `Mute ${s.name}`);
      muteBtn.title = s.muted ? `Unmute ${s.name}` : `Mute ${s.name}`;
      const muteIcon = document.createElement("span");
      muteBtn.append(muteIcon);
      setIcon(muteIcon, s.muted ? "volume-x" : appIcon(s.level, false), { size: 12 });

      slider.addEventListener("input", () => {
        const level = Number(slider.value) / 100;
        pct.textContent = Math.round(level * 100) + "%";
        if (!s.muted) setIcon(muteIcon, appIcon(level, false), { size: 12 });
        invoke("set_app_volume", { id: s.id, level }).catch(() => {});
      });
      slider.addEventListener("wheel", (e) => {
        e.preventDefault();
        const cur = Number(slider.value);
        const next = Math.max(0, Math.min(100, cur + (e.deltaY > 0 ? -5 : 5)));
        slider.value = String(next);
        const level = next / 100;
        pct.textContent = Math.round(level * 100) + "%";
        if (!s.muted) setIcon(muteIcon, appIcon(level, false), { size: 12 });
        invoke("set_app_volume", { id: s.id, level }).catch(() => {});
      }, { passive: false });

      slider.addEventListener("pointerdown", () => dragging.add(s.id));
      const endDrag = () => { dragging.delete(s.id); };
      slider.addEventListener("pointerup", endDrag);
      slider.addEventListener("pointercancel", endDrag);
      slider.addEventListener("pointerleave", endDrag);

      muteBtn.addEventListener("click", () => {
        const nextMuted = !s.muted;
        s.muted = nextMuted;
        row.classList.toggle("is-muted", nextMuted);
        muteBtn.setAttribute("aria-label", nextMuted ? `Unmute ${s.name}` : `Mute ${s.name}`);
        muteBtn.title = nextMuted ? `Unmute ${s.name}` : `Mute ${s.name}`;
        setIcon(muteIcon, nextMuted ? "volume-x" : appIcon(Number(slider.value) / 100, false), { size: 12 });
        invoke("set_app_muted", { id: s.id, muted: nextMuted }).catch(() => {});
      });

      row.append(name, slider, pct, muteBtn);
      rows.push(row);
    }
    mixerBody.replaceChildren(...rows);
  }

  async function refreshSessions(): Promise<void> {
    try {
      const list = await invoke<AppSessionInfo[]>("get_app_sessions");
      // The COM values are authoritative on every refresh; per-session
      // optimistic mute toggles also updated COM, so the next poll
      // reconciles any local state without a flicker (the local click
      // rendered immediately and the COM roundtrip lands within the poll
      // cycle). No prior-state merging is needed.
      sessions = list;
      renderMixer();
      // After the rebuild, refit the window so a freshly-loaded mixer body
      // (which arrives between toggles, e.g. on poll refresh) is not clipped.
      fitWindowToContent();
    } catch {
      /* ignore — common on cold start before an audio client exists */
    }
  }

  // Pull once on open, then refresh modestly. Widgets only need ONE
  // timer each (§13.3); this is the volume popup's single timer.
  let sessionTimer: number | null = null;
  async function startMixerPolling() {
    await refreshSessions();
    if (sessionTimer !== null) return;
    sessionTimer = window.setInterval(async () => {
      // Skip refresh while the popup lost focus (closing) or while a slider
      // is being dragged — the latter avoids the thumb snapping back to a
      // stale value when the COM update has not propagated yet.
      if (dragging.size > 0) return;
      await refreshSessions();
    }, 1500);
  }
  async function stopMixerPolling() {
    if (sessionTimer !== null) {
      clearInterval(sessionTimer);
      sessionTimer = null;
    }
  }

  // Grow/shrink the popup to fit the current content (master row +/
  // - open accordion). Capped at `MAX_POPUP_H`; the accordion body itself
  // scrolls internally (`.vol-mixer__body { max-height ... }`) so the
  // window always has something usable to show.
  function fitWindowToContent(): void {
    requestAnimationFrame(() => {
      // `scrollHeight` gives the natural content height; `Math.min` caps it
      // at the OS-enforced max (which the Rust builder set via
      // `max_inner_size(400, 400)`). Logical units match `inner_size()`.
      const logical = Math.min(MAX_POPUP_H, Math.max(COLLAPSED_H, wrapper.scrollHeight));
      win.setSize(new LogicalSize(POPUP_W, logical)).catch(() => {});
    });
  }

  // When the accordion opens, prime the session list immediately so the
  // user doesn't see "No apps playing audio" for a poll cycle.
  mixer.addEventListener("toggle", () => {
    if (mixer.open && sessions.length === 0) {
      void refreshSessions();
    }
    fitWindowToContent();
  });

  void startMixerPolling();

  // ----- Close on focus loss + Escape (legacy behaviour) -----
  win.onFocusChanged(({ payload }) => {
    if (!payload) {
      void stopMixerPolling();
      win.close().catch(() => {});
    }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      void stopMixerPolling();
      win.close().catch(() => {});
    }
  });

  // ----- Poll for external master volume changes (media keys, etc.) -----
  const masterPoll = setInterval(async () => {
    if (masterDragging) return;
    try {
      const info = await invoke<{ level: number; muted: boolean }>("get_volume");
      masterMuted = info.muted;
      masterSlider.value = String(Math.round(info.level * 100));
      updateMasterIcon(info.level, info.muted);
      updateMasterLabel(info.level);
    } catch { /* ignore */ }
  }, 1000);

  // Stop the master poller on close too so we don't keep touching COM after
  // the window is gone.
  win.onFocusChanged(() => clearInterval(masterPoll));
})();
