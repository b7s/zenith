import "../../styles/bar-globals.css";
import { applyTheme, watchSystemTheme } from "../../shared/window";
import { applyIcons, setIcon } from "../../shared/icon";
import { loadConfig } from "../../shared/config";
import { layoutBar } from "../../shared/widgets";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { initLog, logMemory, logInfo, logError, time } from "../../shared/log";
import { EVENT } from "../../shared/events";
import {
  initArrangeSync,
  isArrangeActive,
  onArrangeChange,
  toggleArrangeMode,
  attachLongPress,
  applyArrangeUI,
  setupBarDropZones,
  setupBarReceiveDrop,
  attachOutsideClickDeactivate,
} from "../../shared/widget-arrange";
import { weatherIcon } from "../../shared/icon";
import type { Config } from "../../shared/types";

void (async () => {
  await initLog();
  logMemory("startup");

  (window as any).__zenith_invoke = invoke;
  (window as any).__zenith_listen = listen;
  (window as any).__zenith_applyIcons = applyIcons;
  (window as any).__zenith_setIcon = setIcon;
  // Used by the weather widget to map OWM condition codes -> Lucide names.
  (window as any).__zenith_weatherIcon = (code: number, icon?: string) =>
    weatherIcon(code, icon);

  // Helper exposed to widget JS: opens the weather popup anchored under
  // the requesting widget element. Widgets call
  // `window.__zenith_openWeather(el)` from their IIFE; we forward through
  // the shared positional helper in `widget-popup.ts`.
  void import("../../shared/widget-popup").then((m) => {
    (window as any).__zenith_openWeather = (el: HTMLElement) => {
      void m.openWeatherFromWidget(el);
    };
    (window as any).__zenith_openCalendar = (el: HTMLElement) => {
      void m.openCalendarFromWidget(el);
    };
  });

  await time("applyTheme", () => applyTheme());
  watchSystemTheme(() => void applyTheme());
  applyIcons();

  const wrapper = document.getElementById("bar-wrapper");
  const bar = document.getElementById("bar");
  if (!wrapper || !bar) {
    logError("bar elements not found");
    return;
  }

  bar.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    void invoke("show_context_menu");
  });

  // Arrange mode: long-press ACTIVATES it (deactivation is via outside-click).
  // Using long-press only to activate prevents it from accidentally turning
  // arrange OFF while the user is pressing a widget to drag it.
  attachLongPress(bar, () => {
    if (!isArrangeActive()) toggleArrangeMode();
  });
  // Click outside widgets (or window blur) deactivates arrange — unless the
  // widget manager is holding it open.
  attachOutsideClickDeactivate();

  let cfg = await time("loadConfig", () => loadConfig());

  // Cross-window sync + re-apply chrome whenever arrange flips.
  void initArrangeSync();
  onArrangeChange(() => applyArrangeUI(bar, cfg));

  applyBarDom(wrapper, bar, cfg);
  try {
    await time("layoutBar", () => layoutBar(bar, cfg));
  } catch (err) {
    logError("initial layoutBar: " + String(err));
  }
  applyArrangeUI(bar, cfg);
  setupBarDropZones(bar, cfg);
  setupBarReceiveDrop(bar, cfg);
  logMemory("after layout");
  logInfo("bar ready");

  listen<Config>(EVENT.configUpdated, async (e) => {
    cfg = e.payload;
    applyTheme();
    applyBarDom(wrapper, bar, cfg);
    try {
      await layoutBar(bar, cfg);
    } catch (err) {
      logError("re-layout: " + String(err));
    }
    applyArrangeUI(bar, cfg);
    logInfo("bar re-applied config");
  });
})();

function applyBarDom(wrapper: HTMLElement, bar: HTMLElement, cfg: Config): void {
  const a = cfg.appearance;
  const barH = Math.max(20, Math.min(200, a.bar_height));
  const totalH = barH + a.margin_top + a.margin_bottom + a.padding_top + a.padding_bottom;
  wrapper.style.height = `${totalH}px`;
  wrapper.style.setProperty("--zen-margin-top", `${a.margin_top}px`);
  wrapper.style.setProperty("--zen-margin-left", `${a.margin_left}px`);
  wrapper.style.setProperty("--zen-margin-right", `${a.margin_right}px`);
  wrapper.style.setProperty("--zen-margin-bottom", `${a.margin_bottom}px`);
  bar.style.setProperty(
    "--zen-corner-radius",
    `${a.corner_radius_tl}px ${a.corner_radius_tr}px ${a.corner_radius_br}px ${a.corner_radius_bl}px`,
  );
  bar.style.padding = `${a.padding_top}px ${a.padding_right}px ${a.padding_bottom}px ${a.padding_left}px`;

  const mode = a.background.mode;
  logInfo(`applyBarDom mode=${mode} color_top=${a.background.color_top} alpha_top=${a.background.alpha_top}`);
  if (mode === "gradient") {
    const topAlpha = (a.background.alpha_top / 100).toFixed(2);
    const botAlpha = (a.background.alpha_bottom / 100).toFixed(2);
    bar.style.background = `linear-gradient(to bottom, ${hexToRgba(a.background.color_top, topAlpha)}, ${hexToRgba(a.background.color_bottom, botAlpha)})`;
  } else if (mode === "solid") {
    const alpha = (a.background.alpha_top / 100).toFixed(2);
    bar.style.background = hexToRgba(a.background.color_top, alpha);
  } else {
    bar.style.background = "";
  }
  logInfo(`applyBarDom background set to: ${bar.style.background}`);
}

function hexToRgba(hex: string, alpha: string): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.substring(0, 2), 16);
  const g = parseInt(h.substring(2, 4), 16);
  const b = parseInt(h.substring(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}
