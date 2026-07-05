import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

void (async () => {
  const { content } = await mountWindow({ title: "Rename Desktop" });

  // Disable text selection and right-click (enable right-click in dev only)
  document.addEventListener("contextmenu", (e) => {
    if (!import.meta.env.DEV) e.preventDefault();
  });
  document.addEventListener("selectstart", (e) => e.preventDefault());

  let desktopId = 0;
  let currentName = "Desktop";
  try {
    const data: [number, string] = await invoke("get_rename_data");
    desktopId = data[0];
    currentName = data[1];
  } catch (e) {
    console.error("[rename] failed to get rename data:", e);
  }

  const field = document.createElement("div");
  field.className = "zen-field";

  const label = document.createElement("label");
  label.className = "zen-label";
  label.textContent = "Desktop name";
  label.htmlFor = "rename-input";
  field.append(label);

  const input = document.createElement("input");
  input.id = "rename-input";
  input.className = "zen-input";
  input.type = "text";
  input.value = currentName;
  input.autofocus = true;
  field.append(input);

  const hint = document.createElement("p");
  hint.className = "zen-hint";
  hint.textContent = "Press Enter to confirm, Esc to cancel.";
  field.append(hint);

  const actions = document.createElement("div");
  actions.style.cssText = "display:flex;gap:0.5rem;justify-content:flex-end;padding:0 1rem 1rem";

  const btnCancel = document.createElement("button");
  btnCancel.className = "zen-button is-outline is-sm";
  btnCancel.textContent = "Cancel";
  actions.append(btnCancel);

  const btnOk = document.createElement("button");
  btnOk.className = "zen-button is-primary is-sm";
  btnOk.textContent = "Rename";
  actions.append(btnOk);

  content.append(field, actions);

  input.select();

  function submit() {
    const name = input.value.trim();
    if (!name) return;
    void invoke("rename_desktop", { id: desktopId, name }).catch((err: unknown) => {
      console.error("[rename] rename error:", err);
    });
    void getCurrentWindow().close();
  }

  btnOk.addEventListener("click", submit);
  btnCancel.addEventListener("click", () => void getCurrentWindow().close());
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); submit(); }
    if (e.key === "Escape") { e.preventDefault(); void getCurrentWindow().close(); }
  });
})();
