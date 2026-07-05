import "../../styles/globals.css";
import { applyTheme } from "../../shared/window";
import { applyIcons } from "../../shared/icon";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

void (async () => {
  try {
    await applyTheme();
    applyIcons();
  } catch (e) {
    console.error("[rename] init error:", e);
  }

  const root = document.getElementById("root");
  if (!root) return;

  let desktopId = 0;
  let currentName = "Desktop";
  try {
    const data: [number, string] = await invoke("get_rename_data");
    desktopId = data[0];
    currentName = data[1];
  } catch (e) {
    console.error("[rename] failed to get rename data:", e);
  }

  root.innerHTML =
    '<div class="zen-window" style="padding:16px;display:flex;flex-direction:column;gap:12px">' +
    '<label class="zen-label" for="rename-input">Desktop name</label>' +
    '<input id="rename-input" class="zen-input" type="text" value="' + escHtml(currentName) + '" autofocus />' +
    '<div style="display:flex;gap:8px;justify-content:flex-end">' +
    '<button id="btn-cancel" class="zen-button is-outline is-sm">Cancel</button>' +
    '<button id="btn-ok" class="zen-button is-primary is-sm">Rename</button>' +
    "</div></div>";

  const input = document.getElementById("rename-input") as HTMLInputElement;
  const btnOk = document.getElementById("btn-ok") as HTMLButtonElement;
  const btnCancel = document.getElementById("btn-cancel") as HTMLButtonElement;

  if (!input || !btnOk || !btnCancel) return;

  input.select();

  function submit() {
    const name = input.value.trim();
    if (!name) return;
    invoke("rename_desktop", { id: desktopId, name }).catch((err: any) => {
      console.error("[rename] rename error:", err);
    });
    void getCurrentWindow().close();
  }

  btnOk.addEventListener("click", submit);
  btnCancel.addEventListener("click", () => void getCurrentWindow().close());
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") submit();
    if (e.key === "Escape") void getCurrentWindow().close();
  });
})();

function escHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}
