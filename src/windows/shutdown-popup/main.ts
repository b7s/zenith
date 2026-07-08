import "../../styles/globals.css";
import "./shutdown.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { mountWindow } from "../../shared/window";
import { initLog, logInfo } from "../../shared/log";
import { setIcon } from "../../shared/icon";

interface ActionDef {
  id: string;
  label: string;
  icon: string;
  command: string;
  destructive?: boolean;
  noConfirm?: boolean;
}

const origActionMap = new WeakMap<HTMLElement, ActionDef>();

const ACTIONS: ActionDef[] = [
  { id: "shutdown", label: "Shut Down", icon: "power", command: "system_shutdown", destructive: true },
  { id: "restart", label: "Restart", icon: "refresh-cw", command: "system_restart" },
  { id: "sleep", label: "Sleep", icon: "moon", command: "system_sleep" },
  { id: "hibernate", label: "Hibernate", icon: "cloud-moon", command: "system_hibernate" },
  { id: "lock", label: "Lock", icon: "lock", command: "system_lock", noConfirm: true },
  { id: "logout", label: "Log Out", icon: "log-out", command: "system_logout" },
];

const CONFIRM_TIMEOUT_MS = 4_000;

let confirmingEl: HTMLElement | null = null;
let confirmTimer: ReturnType<typeof setTimeout> | null = null;

function resetConfirm() {
  if (confirmTimer) { clearTimeout(confirmTimer); confirmTimer = null; }
  if (!confirmingEl) return;
  confirmingEl.classList.remove("is-confirming");
  const iconEl = confirmingEl.querySelector(".sd-icon") as HTMLElement | null;
  const labelEl = confirmingEl.querySelector(".sd-label") as HTMLElement | null;
  const orig = origActionMap.get(confirmingEl);
  if (iconEl && orig) setIcon(iconEl, orig.icon, { size: 22 });
  if (labelEl && orig) labelEl.textContent = orig.label;
  confirmingEl = null;
}

const grid = document.createElement("div");
grid.className = "sd-grid";

function buildButton(action: ActionDef): HTMLElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "zen-button is-lg" + (action.destructive ? " is-destructive" : "");
  btn.classList.add("sd-btn");
  origActionMap.set(btn, action);

  const iconEl = document.createElement("span");
  iconEl.className = "sd-icon";
  setIcon(iconEl, action.icon, { size: 22 });
  btn.append(iconEl);

  const labelEl = document.createElement("span");
  labelEl.className = "sd-label";
  labelEl.textContent = action.label;
  btn.append(labelEl);

  btn.addEventListener("click", async () => {
    if (action.noConfirm) {
      try { await invoke(action.command); } catch (err) { logInfo(`${action.id} failed: ${err}`); }
      return;
    }
    if (confirmingEl === btn) {
      try {
        await invoke(action.command);
      } catch (err) {
        logInfo(`${action.id} failed: ${err}`);
      }
      return;
    }
    resetConfirm();
    confirmingEl = btn;
    btn.classList.add("is-confirming");
    setIcon(iconEl, "triangle-alert", { size: 22 });
    labelEl.textContent = "Confirm?";
    confirmTimer = setTimeout(resetConfirm, CONFIRM_TIMEOUT_MS);
  });

  return btn;
}

void (async () => {
  await initLog();
  logInfo("shutdown popup ready");

  const { content } = await mountWindow({ title: "Shutdown" });

  content.style.cssText =
    "display:flex;flex-direction:column;padding:0.75rem;height:100%;overflow:hidden;";

  for (const action of ACTIONS) grid.append(buildButton(action));
  content.append(grid);

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { resetConfirm(); void getCurrentWindow().close().catch(() => {}); }
  });

  const win = getCurrentWindow();
  void win.onFocusChanged(({ payload }) => {
    if (payload === false) { resetConfirm(); void win.close().catch(() => {}); }
  });
})();
