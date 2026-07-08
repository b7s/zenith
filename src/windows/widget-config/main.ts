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

  const cfg = await invoke<Config>("get_config");
  const manifests = await invoke<WidgetManifest[]>("get_widgets");
  const manifest = manifests.find((m) => m.id === widgetId);

  if (!manifest) {
    logInfo(`manifest not found for widget id: ${widgetId}`);
    const { content } = await mountWindow({ title: "Widget Settings" });
    const hint = document.createElement("p");
    hint.className = "zen-hint";
    hint.style.padding = "1rem";
    hint.textContent = "Widget not found.";
    content.append(hint);
    return;
  }

  const configKeys = manifest.config ? Object.keys(manifest.config) : [];
  logInfo(`widget=${widgetId} config keys=[${configKeys.join(", ")}]`);

  if (!manifest.config || configKeys.length === 0) {
    const { content } = await mountWindow({ title: "Widget Settings" });
    const hint = document.createElement("p");
    hint.className = "zen-hint";
    hint.style.padding = "1rem";
    content.append(hint);
    return;
  }

  const savedValues =
    (cfg.widgets.config?.[widgetId] as Record<string, unknown> | undefined) ?? {};

  const configDef = manifest.config ?? {};

  const form = document.createElement("div");
  form.className = "zen-section";

  const inputs: Record<string, HTMLElement> = {};
  const switchStates: Record<string, boolean> = {};

  // Dynamic hardware selection state (for system_stats)
  let selectedGpus: string[] = [];
  let selectedHds: string[] = [];
  let selectedNetworks: string[] = [];

  for (const [key, field] of Object.entries(configDef)) {
    const wrapper = document.createElement("div");
    wrapper.className = "zen-field";

    const currentValue = key in savedValues ? savedValues[key] : field.value;

    if (field.type === "bool") {
      buildBoolControl(wrapper, key, field, currentValue, switchStates);
    } else {
      const label = document.createElement("label");
      label.className = "zen-label";
      label.textContent = field.label || key;
      wrapper.append(label);
      buildControl(wrapper, key, field, currentValue, inputs);
      if (field.hint) {
        const hint = document.createElement("p");
        hint.className = "zen-hint";
        hint.textContent = field.hint;
        wrapper.append(hint);
      }
    }

    form.append(wrapper);
  }

  // For system_stats widget: show per-GPU and per-drive checkboxes
  if (widgetId === "system_stats") {
    try {
      const stats = await invoke<{
        gpu: { name: string; percent: number }[];
        hd: { mount: string; used: number; total: number; percent: number }[];
        network: { name: string; recv_bps: number; send_bps: number }[];
      }>("get_system_stats");

      const prevSelected = (savedValues.selected_gpus as string[] | undefined);
      const prevSelectedHd = (savedValues.selected_hds as string[] | undefined);
      const prevSelectedNet = (savedValues.selected_networks as string[] | undefined);
      const isFirstTime = !prevSelected && !prevSelectedHd && !prevSelectedNet;

      if (stats.gpu.length > 0) {
        const gpuSection = document.createElement("div");
        gpuSection.className = "zen-field";
        gpuSection.style.marginTop = "0.5rem";

        const gpuLabel = document.createElement("label");
        gpuLabel.className = "zen-label";
        gpuLabel.textContent = "GPUs to show";
        gpuSection.append(gpuLabel);

        for (let gi = 0; gi < stats.gpu.length; gi++) {
          const g = stats.gpu[gi];
          const checked = isFirstTime
            ? gi === 0
            : prevSelected
              ? prevSelected.includes(g.name)
              : true;
          if (checked) selectedGpus.push(g.name);
          buildHwCheckbox(gpuSection, g.name, checked, (isChecked) => {
            if (isChecked && !selectedGpus.includes(g.name)) selectedGpus.push(g.name);
            if (!isChecked) selectedGpus = selectedGpus.filter((x) => x !== g.name);
          });
        }

        form.append(gpuSection);
      }

      if (stats.hd.length > 0) {
        const hdSection = document.createElement("div");
        hdSection.className = "zen-field";
        hdSection.style.marginTop = "0.5rem";

        const hdLabel = document.createElement("label");
        hdLabel.className = "zen-label";
        hdLabel.textContent = "Drives to show";
        hdSection.append(hdLabel);

        const sysDrive = "C:";
        for (let hi = 0; hi < stats.hd.length; hi++) {
          const h = stats.hd[hi];
          const checked = isFirstTime
            ? h.mount === sysDrive
            : prevSelectedHd
              ? prevSelectedHd.includes(h.mount)
              : true;
          if (checked) selectedHds.push(h.mount);
          buildHwCheckbox(hdSection, h.mount, checked, (isChecked) => {
            if (isChecked && !selectedHds.includes(h.mount)) selectedHds.push(h.mount);
            if (!isChecked) selectedHds = selectedHds.filter((x) => x !== h.mount);
          });
        }

        form.append(hdSection);
      }

      if (stats.network && stats.network.length > 0) {
        const netSection = document.createElement("details");
        netSection.className = "zen-collapse";
        netSection.style.marginTop = "0.5rem";

        const netSummary = document.createElement("summary");
        netSummary.textContent = `Adapters to show (${stats.network.length})`;
        netSection.append(netSummary);

        const netBody = document.createElement("div");
        netBody.style.marginTop = "0.25rem";
        netBody.style.display = "grid";
        netBody.style.gap = "0.5rem";

        for (let ni = 0; ni < stats.network.length; ni++) {
          const n = stats.network[ni];
          const checked = isFirstTime
            ? ni === 0
            : prevSelectedNet
              ? prevSelectedNet.includes(n.name)
              : true;
          if (checked) selectedNetworks.push(n.name);
          buildHwCheckbox(netBody, n.name, checked, (isChecked) => {
            if (isChecked && !selectedNetworks.includes(n.name)) selectedNetworks.push(n.name);
            if (!isChecked) selectedNetworks = selectedNetworks.filter((x) => x !== n.name);
          });
        }

        netSection.append(netBody);
        form.append(netSection);
      }
    } catch {
      // ignore — hardware checkboxes won't show
    }
  }

  // Footer actions
  const footer = document.createElement("div");
  footer.style.cssText = "display:flex;gap:0.5rem;justify-content:flex-end;";

  const cancelBtn = document.createElement("button");
  cancelBtn.type = "button";
  cancelBtn.className = "zen-button is-outline";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  });
  footer.append(cancelBtn);

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
        const el = inputs[key];
        const val = el instanceof HTMLInputElement ? el.value : "0";
        newValues[key] = parseInt(val, 10) || 0;
      } else if (field.type === "select") {
        const el = inputs[key];
        if (field.options && field.options.length <= 3) {
          const checked = el?.querySelector?.("input:checked") as HTMLInputElement | null;
          newValues[key] = checked?.value ?? field.value;
        } else {
          newValues[key] = (el as HTMLSelectElement | null)?.value ?? field.value;
        }
      } else {
        newValues[key] = (inputs[key] as HTMLInputElement | null)?.value ?? field.value;
      }
    }
    if (!cfg.widgets.config) cfg.widgets.config = {};
    cfg.widgets.config[widgetId] = newValues;
    if (widgetId === "system_stats") {
      (cfg.widgets.config[widgetId] as Record<string, unknown>).selected_gpus = selectedGpus;
      (cfg.widgets.config[widgetId] as Record<string, unknown>).selected_hds = selectedHds;
      (cfg.widgets.config[widgetId] as Record<string, unknown>).selected_networks = selectedNetworks;
    }
    await invoke("save_config", { config: cfg });
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  });
  footer.append(saveBtn);

  const { content } = await mountWindow({ title: "Widget Settings", footer });

  const wrapper = document.createElement("div");
  wrapper.style.cssText = "flex:1;overflow-y:auto;overflow-x:hidden;padding:1rem;";
  content.append(wrapper);
  wrapper.append(form);
})();

// Close on Escape.
document.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  }
});

function buildControl(
  wrapper: HTMLElement,
  key: string,
  field: WidgetConfigField,
  currentValue: unknown,
  inputs: Record<string, HTMLElement>,
): void {
  if (field.type === "select" && field.options) {
    if (field.options.length <= 3) {
      const group = document.createElement("div");
      group.className = "zen-radio-group";
      for (const opt of field.options) {
        const label = document.createElement("label");
        label.className = "zen-radio-card";
        if (String(opt) === String(currentValue)) label.classList.add("is-selected");
        const radio = document.createElement("input");
        radio.type = "radio";
        radio.name = `config-${key}`;
        radio.value = String(opt);
        if (String(opt) === String(currentValue)) radio.checked = true;
        radio.addEventListener("change", () => {
          group.querySelectorAll(".zen-radio-card").forEach((c) => c.classList.remove("is-selected"));
          label.classList.add("is-selected");
        });
        label.append(radio);
        const span = document.createElement("span");
        span.textContent = String(opt);
        label.append(span);
        group.append(label);
      }
      inputs[key] = group;
      wrapper.append(group);
    } else {
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
    }
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

function buildBoolControl(
  wrapper: HTMLElement,
  key: string,
  field: WidgetConfigField,
  currentValue: unknown,
  switchStates: Record<string, boolean>,
): void {
  const checkbox = document.createElement("label");
  checkbox.className = "zen-checkbox";

  const text = document.createElement("span");
  text.className = "zen-checkbox__text";

  const label = document.createElement("span");
  label.className = "zen-checkbox__label";
  label.textContent = field.label || key;
  text.append(label);

  if (field.hint) {
    const desc = document.createElement("span");
    desc.className = "zen-checkbox__desc";
    desc.textContent = field.hint;
    text.append(desc);
  }

  checkbox.append(text);

  const switchEl = document.createElement("span");
  switchEl.className = "zen-checkbox__switch";

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = Boolean(currentValue);
  switchStates[key] = input.checked;
  input.addEventListener("change", () => {
    switchStates[key] = input.checked;
  });
  switchEl.append(input);

  const track = document.createElement("span");
  track.className = "zen-checkbox__track";
  const thumb = document.createElement("span");
  thumb.className = "zen-checkbox__thumb";
  track.append(thumb);
  switchEl.append(track);

  checkbox.append(switchEl);
  wrapper.append(checkbox);
}

function buildHwCheckbox(
  wrapper: HTMLElement,
  name: string,
  checked: boolean,
  onChange: (isChecked: boolean) => void,
): void {
  const checkbox = document.createElement("label");
  checkbox.className = "zen-checkbox";

  const text = document.createElement("span");
  text.className = "zen-checkbox__text";

  const label = document.createElement("span");
  label.className = "zen-checkbox__label";
  label.textContent = name;
  text.append(label);

  checkbox.append(text);

  const switchEl = document.createElement("span");
  switchEl.className = "zen-checkbox__switch";

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  input.addEventListener("change", () => {
    onChange(input.checked);
  });
  switchEl.append(input);

  const track = document.createElement("span");
  track.className = "zen-checkbox__track";
  const thumb = document.createElement("span");
  thumb.className = "zen-checkbox__thumb";
  track.append(thumb);
  switchEl.append(track);

  checkbox.append(switchEl);
  wrapper.append(checkbox);
}
