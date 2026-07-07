import "../../styles/globals.css";
import { invoke } from "@tauri-apps/api/core";
import { mountWindow } from "../../shared/window";
import { initLog, logInfo } from "../../shared/log";
import type { Config, WidgetManifest, WidgetConfigField } from "../../shared/types";

interface WidgetConfigGlobals {
  __ZENITH_WIDGET_CONFIG_ID: string;
}

void (async () => {
  await initLog();

  const widgetId =
    (window as unknown as Partial<WidgetConfigGlobals>).__ZENITH_WIDGET_CONFIG_ID ?? "";

  if (!widgetId) {
    logInfo("no widget id provided");
    return;
  }

  const { content } = await mountWindow({ title: "Widget Settings" });

  const cfg = await invoke<Config>("get_config");
  const manifests = await invoke<WidgetManifest[]>("get_widgets");
  const manifest = manifests.find((m) => m.id === widgetId);

  if (!manifest || !manifest.config || Object.keys(manifest.config).length === 0) {
    const hint = document.createElement("p");
    hint.className = "zen-hint";
    hint.style.padding = "1rem";
    hint.textContent = "This widget has no configurable settings.";
    content.append(hint);
    return;
  }

  const savedValues =
    (cfg.widgets.config?.[widgetId] as Record<string, unknown> | undefined) ?? {};

  const configDef = manifest.config ?? {};

  const form = document.createElement("div");
  form.className = "zen-section";
  content.append(form);

  const inputs: Record<string, HTMLInputElement | HTMLSelectElement> = {};
  const switchStates: Record<string, boolean> = {};

  for (const [key, field] of Object.entries(configDef)) {
    const wrapper = document.createElement("div");
    wrapper.className = "zen-field";

    const label = document.createElement("label");
    label.className = "zen-label";
    label.textContent = field.label || key;
    wrapper.append(label);

    const currentValue = key in savedValues ? savedValues[key] : field.value;

    buildControl(wrapper, key, field, currentValue, inputs, switchStates);

    if (field.hint) {
      const hint = document.createElement("p");
      hint.className = "zen-hint";
      hint.textContent = field.hint;
      wrapper.append(hint);
    }

    form.append(wrapper);
  }

  const actions = document.createElement("div");
  actions.style.cssText =
    "display:flex;gap:0.5rem;justify-content:flex-end;margin-top:1rem;";
  form.append(actions);

  const cancelBtn = document.createElement("button");
  cancelBtn.type = "button";
  cancelBtn.className = "zen-button is-outline";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  });
  actions.append(cancelBtn);

  const saveBtn = document.createElement("button");
  saveBtn.type = "button";
  saveBtn.className = "zen-button is-primary";
  saveBtn.textContent = "Save";
  saveBtn.addEventListener("click", async () => {
    const newValues: Record<string, unknown> = {};
    for (const [key, field] of Object.entries(configDef)) {
      if (field.type === "bool") {
        newValues[key] = switchStates[key] ?? false;
      } else if (field.type === "int") {
        newValues[key] = parseInt(inputs[key]?.value ?? "0", 10) || 0;
      } else if (field.type === "select") {
        newValues[key] = inputs[key]?.value ?? field.value;
      } else {
        newValues[key] = inputs[key]?.value ?? field.value;
      }
    }
    if (!cfg.widgets.config) cfg.widgets.config = {};
    cfg.widgets.config[widgetId] = newValues;
    await invoke("save_config", { config: cfg });
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  });
  actions.append(saveBtn);
})();

function buildControl(
  wrapper: HTMLElement,
  key: string,
  field: WidgetConfigField,
  currentValue: unknown,
  inputs: Record<string, HTMLInputElement | HTMLSelectElement>,
  switchStates: Record<string, boolean>,
): void {
  if (field.type === "select" && field.options) {
    const select = document.createElement("select");
    select.className = "zen-select";
    for (const opt of field.options) {
      const option = document.createElement("option");
      option.value = String(opt);
      option.textContent = String(opt);
      if (String(opt) === String(currentValue)) option.selected = true;
      select.append(option);
    }
    inputs[key] = select;
    const selectWrapper = document.createElement("div");
    selectWrapper.className = "zen-select-wrapper";
    selectWrapper.append(select);
    wrapper.append(selectWrapper);
  } else if (field.type === "bool") {
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "zen-checkbox";
    checkbox.checked = Boolean(currentValue);
    switchStates[key] = checkbox.checked;
    checkbox.addEventListener("change", () => {
      switchStates[key] = checkbox.checked;
    });
    wrapper.append(checkbox);
  } else if (field.type === "int") {
    const input = document.createElement("input");
    input.type = "number";
    input.className = "zen-input";
    input.value = String(currentValue ?? 0);
    inputs[key] = input;
    wrapper.append(input);
  } else {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "zen-input";
    input.value = String(currentValue ?? "");
    inputs[key] = input;
    wrapper.append(input);
  }
}
