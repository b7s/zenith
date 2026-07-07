import "../../styles/globals.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setIcon } from "../../shared/icon";
import { applyTheme } from "../../shared/window";
import { initLog, logInfo } from "../../shared/log";

void (async () => {
  await initLog();

  applyTheme();

  const root = document.getElementById("root");
  if (!root) return;

  root.style.cssText =
    "display:flex;align-items:center;gap:6px;padding:0 12px;height:100%;overflow:hidden;";

  const iconEl = document.createElement("span");
  iconEl.style.cssText = "flex-shrink:0;display:flex;align-items:center;";

  const slider = document.createElement("input");
  slider.type = "range";
  slider.min = "0";
  slider.max = "100";
  slider.value = "50";
  slider.className = "zen-slider";
  slider.style.cssText = "flex:1;min-width:0;";

  const label = document.createElement("span");
  label.style.cssText =
    "flex-shrink:0;width:36px;text-align:right;font-size:0.75rem;font-variant-numeric:tabular-nums;" +
    "color:var(--muted-foreground);";

  root.append(iconEl, slider, label);

  function updateIcon(level: number, muted: boolean) {
    let name: string;
    if (muted || level < 0.01) name = "volume-x";
    else if (level < 0.50) name = "volume-1";
    else name = "volume-2";
    setIcon(iconEl, name, { size: 14 });
  }

  function updateLabel(level: number) {
    label.textContent = Math.round(level * 100) + "%";
  }

  // Load initial volume
  try {
    const info = await invoke<{ level: number; muted: boolean }>("get_volume");
    const level = info.level;
    const muted = info.muted;
    slider.value = String(Math.round(level * 100));
    updateIcon(level, muted);
    updateLabel(level);
  } catch {
    logInfo("failed to get initial volume");
  }

  // Live slider
  slider.addEventListener("input", () => {
    const level = Number(slider.value) / 100;
    updateIcon(level, false);
    updateLabel(level);
    invoke("set_volume", { level }).catch(() => {});
  });

  // Scroll over slider to change volume
  slider.addEventListener("wheel", (e) => {
    e.preventDefault();
    const cur = Number(slider.value);
    const step = e.deltaY > 0 ? -5 : 5;
    const next = Math.max(0, Math.min(100, cur + step));
    slider.value = String(next);
    const level = next / 100;
    updateIcon(level, false);
    updateLabel(level);
    invoke("set_volume", { level }).catch(() => {});
  }, { passive: false });

  // Close on blur (focus loss)
  const win = getCurrentWindow();
  win.onFocusChanged(({ payload }) => {
    if (!payload) {
      win.close().catch(() => {});
    }
  });

  // Also close on Escape
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      win.close().catch(() => {});
    }
  });
})();
