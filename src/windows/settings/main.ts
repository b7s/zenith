import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";
import { mountTabs } from "../../shared/tabs";
import { setIcon } from "../../shared/icon";
import { loadConfig, saveConfig } from "../../shared/config";
import { initLog, logMemory, logInfo } from "../../shared/log";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type { Config, AppearanceConfig, BackgroundMode, UpdateStatus } from "../../shared/types";
import { CMD } from "../../shared/ipc";
import { EVENT } from "../../shared/events";

void (async () => {
  await initLog();
  logMemory("startup");

  const { content } = await mountWindow({ title: "Settings" });

  const tabs = mountTabs(content, [
    { id: "general", label: "General" },
    { id: "bar", label: "Bar" },
    { id: "about", label: "About" },
  ]);

  const widgetsLink = document.createElement("button");
  widgetsLink.className = "zen-tab zen-tab--action";
  widgetsLink.textContent = "Widgets";
  const linkIcon = document.createElement("span");
  linkIcon.className = "zen-icon";
  linkIcon.style.marginLeft = "0.25rem";
  setIcon(linkIcon, "external-link", { size: 12 });
  widgetsLink.append(linkIcon);
  widgetsLink.addEventListener("click", () => void invoke(CMD.openWidgets));
  tabs.container.append(widgetsLink);

  content.prepend(tabs.container);

  let config: Config = await loadConfig();

  buildGeneralTab(tabs.panes.general, (patch: Partial<Config>) => {
    config = { ...config, updates: { ...config.updates, ...patch.updates } };
    void saveConfig(config);
  });

  buildBarTab(tabs.panes.bar, (patch) => {
    config = { ...config, appearance: { ...config.appearance, ...patch } };
    void saveConfig(config);
  });

  buildAboutTab(tabs.panes.about);

  listen<Config>(EVENT.configUpdated, (e) => {
    config = e.payload;
  });

  logMemory("after mount");
  logInfo("settings ready");

  // --- helpers ---
  function field(label: string): HTMLDivElement {
    const el = document.createElement("div");
    el.className = "zen-field";
    const lbl = document.createElement("label");
    lbl.className = "zen-label";
    lbl.textContent = label;
    el.append(lbl);
    return el;
  }

  function hint(text: string): HTMLSpanElement {
    const s = document.createElement("span");
    s.className = "zen-hint";
    s.textContent = text;
    return s;
  }

  function valueRow(slider: HTMLInputElement, display: HTMLElement): HTMLDivElement {
    const r = document.createElement("div");
    r.style.cssText = "display:flex;align-items:center;gap:0.75rem";
    r.append(slider, display);
    return r;
  }

  function radioGroup(
    name: string,
    values: readonly string[],
    current: string,
    setting: string,
    onSel: (v: string) => void
  ): HTMLDivElement {
    const group = document.createElement("div");
    group.className = "zen-radio-group";
    for (const val of values) {
      const card = document.createElement("label");
      card.className = `zen-radio-card${current === val ? " is-selected" : ""}`;
      const radio = document.createElement("input");
      radio.type = "radio";
      radio.name = name;
      radio.value = val;
      radio.checked = current === val;
      radio.dataset.setting = setting;
      card.append(radio, val.charAt(0).toUpperCase() + val.slice(1));
      card.addEventListener("click", () => {
        group.querySelectorAll(".zen-radio-card").forEach((c) => c.classList.remove("is-selected"));
        card.classList.add("is-selected");
        onSel(val);
      });
      group.append(card);
    }
    return group;
  }

  function buildBarTab(
    pane: HTMLElement,
    update: (patch: Partial<AppearanceConfig>) => void
  ) {
    const section = document.createElement("div");
    section.className = "zen-section";

    const bgModes = ["acrylic", "mica", "solid", "gradient", "none"] as const;

    // Use a helper to always read the LATEST background from the outer config
    function bg() {
      return config.appearance.background;
    }

    // --- Theme ---
    const themeField = field("Theme");
    themeField.append(radioGroup("theme", ["auto", "dark", "light"], config.appearance.theme, "theme", (v) => {
      update({ theme: v as AppearanceConfig["theme"] });
    }));
    section.append(themeField);

    // --- Background (unified) ---
    const bgField = field("Background");
    const bgGroup = radioGroup("bg_mode", bgModes, bg().mode, "bg_mode", (v) => {
      update({ background: { ...bg(), mode: v as BackgroundMode } });
      toggleSections();
    });
    bgField.append(bgGroup);
    section.append(bgField);

    // --- Tint opacity (acrylic/mica only) ---
    const tintField = field("Tint opacity");
    const tintSlider = document.createElement("input");
    tintSlider.type = "range";
    tintSlider.className = "zen-slider";
    tintSlider.min = "0";
    tintSlider.max = "255";
    tintSlider.value = String(config.appearance.tint_alpha);
    tintSlider.dataset.setting = "tint_alpha";
    const tintValue = hint(String(config.appearance.tint_alpha));
    tintSlider.addEventListener("input", () => { tintValue.textContent = tintSlider.value; });
    tintField.append(valueRow(tintSlider, tintValue));
    section.append(tintField);

    // --- Divider before colors ---
    const d1 = document.createElement("hr");
    d1.className = "zen-divider";
    section.append(d1);

    // --- Color top + alpha ---
    const topField = field("Color top");
    const topColor = document.createElement("input");
    topColor.type = "color";
    topColor.value = bg().color_top;
    topColor.dataset.setting = "color_top";
    topColor.style.cssText = "width:2rem;height:2rem;padding:0;border:0;cursor:pointer;background:none";
    const topAlpha = document.createElement("input");
    topAlpha.type = "range";
    topAlpha.className = "zen-slider";
    topAlpha.min = "0";
    topAlpha.max = "100";
    topAlpha.value = String(bg().alpha_top);
    topAlpha.dataset.setting = "alpha_top";
    topAlpha.style.maxWidth = "8rem";
    const topAlphaVal = hint(`${topAlpha.value}%`);
    topAlpha.addEventListener("input", () => { topAlphaVal.textContent = `${topAlpha.value}%`; });
    const topRow = document.createElement("div");
    topRow.style.cssText = "display:flex;align-items:center;gap:0.75rem";
    topRow.append(topColor, topAlpha, topAlphaVal);
    topField.append(topRow);
    section.append(topField);

    // --- Color bottom + alpha (gradient only) ---
    const bottomField = field("Color bottom");
    const bottomColor = document.createElement("input");
    bottomColor.type = "color";
    bottomColor.value = bg().color_bottom;
    bottomColor.dataset.setting = "color_bottom";
    bottomColor.style.cssText = "width:2rem;height:2rem;padding:0;border:0;cursor:pointer;background:none";
    const bottomAlpha = document.createElement("input");
    bottomAlpha.type = "range";
    bottomAlpha.className = "zen-slider";
    bottomAlpha.min = "0";
    bottomAlpha.max = "100";
    bottomAlpha.value = String(bg().alpha_bottom);
    bottomAlpha.dataset.setting = "alpha_bottom";
    bottomAlpha.style.maxWidth = "8rem";
    const bottomAlphaVal = hint(`${bottomAlpha.value}%`);
    bottomAlpha.addEventListener("input", () => { bottomAlphaVal.textContent = `${bottomAlpha.value}%`; });
    const bottomRow = document.createElement("div");
    bottomRow.style.cssText = "display:flex;align-items:center;gap:0.75rem";
    bottomRow.append(bottomColor, bottomAlpha, bottomAlphaVal);
    bottomField.append(bottomRow);
    section.append(bottomField);

    // --- Divider ---
    const d2 = document.createElement("hr");
    d2.className = "zen-divider";
    section.append(d2);

    // --- Bar height ---
    const heightField = field("Bar height");
    const heightSlider = document.createElement("input");
    heightSlider.type = "range";
    heightSlider.className = "zen-slider";
    heightSlider.min = "28";
    heightSlider.max = "72";
    heightSlider.value = String(config.appearance.bar_height);
    heightSlider.dataset.setting = "bar_height";
    const heightValue = hint(`${config.appearance.bar_height}px`);
    heightSlider.addEventListener("input", () => { heightValue.textContent = `${heightSlider.value}px`; });
    heightField.append(valueRow(heightSlider, heightValue));
    section.append(heightField);

    // --- Corner radius (per corner) ---
    const cornerField = field("Corner radius (px)");
    const cornerGrid = document.createElement("div");
    cornerGrid.style.cssText = "display:grid;grid-template-columns:1fr 1fr 1fr 1fr;gap:0.5rem";
    for (const cf of [
      { label: "TL", key: "corner_radius_tl", value: config.appearance.corner_radius_tl },
      { label: "TR", key: "corner_radius_tr", value: config.appearance.corner_radius_tr },
      { label: "BR", key: "corner_radius_br", value: config.appearance.corner_radius_br },
      { label: "BL", key: "corner_radius_bl", value: config.appearance.corner_radius_bl },
    ] as const) {
      const wrapper = document.createElement("div");
      wrapper.style.cssText = "display:flex;flex-direction:column;gap:0.25rem";
      const lbl = document.createElement("span");
      lbl.className = "zen-hint";
      lbl.textContent = cf.label;
      const inp = document.createElement("input");
      inp.type = "number";
      inp.className = "zen-input";
      inp.min = "0";
      inp.max = "40";
      inp.value = String(cf.value);
      inp.dataset.setting = cf.key;
      inp.style.height = "2rem";
      wrapper.append(lbl, inp);
      cornerGrid.append(wrapper);
    }
    cornerField.append(cornerGrid);
    section.append(cornerField);

    // --- Margins ---
    const marginField = field("Margins (px)");
    const marginGrid = document.createElement("div");
    marginGrid.style.cssText = "display:grid;grid-template-columns:1fr 1fr 1fr 1fr;gap:0.5rem";
    const a = config.appearance;
    for (const mf of [
      { label: "Top", key: "margin_top", value: a.margin_top },
      { label: "Left", key: "margin_left", value: a.margin_left },
      { label: "Right", key: "margin_right", value: a.margin_right },
      { label: "Bottom", key: "margin_bottom", value: a.margin_bottom },
    ] as const) {
      const wrapper = document.createElement("div");
      wrapper.style.cssText = "display:flex;flex-direction:column;gap:0.25rem";
      const lbl = document.createElement("span");
      lbl.className = "zen-hint";
      lbl.textContent = mf.label;
      const inp = document.createElement("input");
      inp.type = "number";
      inp.className = "zen-input";
      inp.min = "0";
      inp.value = String(mf.value);
      inp.dataset.setting = mf.key;
      inp.style.height = "2rem";
      wrapper.append(lbl, inp);
      marginGrid.append(wrapper);
    }
    marginField.append(marginGrid);
    section.append(marginField);

    // --- Padding ---
    const paddingField = field("Padding (px)");
    const paddingGrid = document.createElement("div");
    paddingGrid.style.cssText = "display:grid;grid-template-columns:1fr 1fr 1fr 1fr;gap:0.5rem";
    for (const pf of [
      { label: "Top", key: "padding_top", value: a.padding_top },
      { label: "Left", key: "padding_left", value: a.padding_left },
      { label: "Right", key: "padding_right", value: a.padding_right },
      { label: "Bottom", key: "padding_bottom", value: a.padding_bottom },
    ] as const) {
      const wrapper = document.createElement("div");
      wrapper.style.cssText = "display:flex;flex-direction:column;gap:0.25rem";
      const lbl = document.createElement("span");
      lbl.className = "zen-hint";
      lbl.textContent = pf.label;
      const inp = document.createElement("input");
      inp.type = "number";
      inp.className = "zen-input";
      inp.min = "0";
      inp.value = String(pf.value);
      inp.dataset.setting = pf.key;
      inp.style.height = "2rem";
      wrapper.append(lbl, inp);
      paddingGrid.append(wrapper);
    }
    paddingField.append(paddingGrid);
    section.append(paddingField);

    pane.append(section);

    // Toggle visibility of sections based on background mode
    function toggleSections() {
      const mode = bgGroup.querySelector<HTMLInputElement>("input:checked")?.value;
      const blur = mode === "acrylic" || mode === "mica";
      const colors = mode === "solid" || mode === "gradient";
      const gradient = mode === "gradient";
      tintField.style.display = blur ? "" : "none";
      d1.style.display = colors ? "" : "none";
      topField.style.display = colors ? "" : "none";
      topField.querySelector("label")!.textContent = gradient ? "Color top" : "Color";
      bottomField.style.display = gradient ? "" : "none";
    }
    toggleSections();

    // --- Event delegation ---
    pane.addEventListener("change", (e) => {
      const target = e.target as HTMLInputElement;
      const setting = target.dataset.setting;
      if (!setting) return;

      if (setting === "theme") {
        update({ theme: target.value as AppearanceConfig["theme"] });
      } else if (setting === "bg_mode") {
        update({ background: { ...bg(), mode: target.value as BackgroundMode } });
      } else if (setting === "color_top") {
        update({ background: { ...bg(), color_top: target.value } });
      } else if (setting === "color_bottom") {
        update({ background: { ...bg(), color_bottom: target.value } });
      }
    });

    pane.addEventListener("input", (e) => {
      const target = e.target as HTMLInputElement;
      const setting = target.dataset.setting;
      if (!setting) return;

      if (setting === "tint_alpha") {
        update({ tint_alpha: Number(target.value) });
      } else if (setting === "corner_radius_tl") {
        update({ corner_radius_tl: Number(target.value) });
      } else if (setting === "corner_radius_tr") {
        update({ corner_radius_tr: Number(target.value) });
      } else if (setting === "corner_radius_br") {
        update({ corner_radius_br: Number(target.value) });
      } else if (setting === "corner_radius_bl") {
        update({ corner_radius_bl: Number(target.value) });
      } else if (setting === "bar_height") {
        update({ bar_height: Number(target.value) });
      } else if (setting === "margin_top") {
        update({ margin_top: Number(target.value) });
      } else if (setting === "margin_left") {
        update({ margin_left: Number(target.value) });
      } else if (setting === "margin_right") {
        update({ margin_right: Number(target.value) });
      } else if (setting === "margin_bottom") {
        update({ margin_bottom: Number(target.value) });
      } else if (setting === "padding_top") {
        update({ padding_top: Number(target.value) });
      } else if (setting === "padding_left") {
        update({ padding_left: Number(target.value) });
      } else if (setting === "padding_right") {
        update({ padding_right: Number(target.value) });
      } else if (setting === "padding_bottom") {
        update({ padding_bottom: Number(target.value) });
      } else if (setting === "alpha_top") {
        update({ background: { ...bg(), alpha_top: Number(target.value) } });
      } else if (setting === "alpha_bottom") {
        update({ background: { ...bg(), alpha_bottom: Number(target.value) } });
      }
    });
  }

  async function buildAboutTab(pane: HTMLElement) {
    const section = document.createElement("div");
    section.className = "zen-section";
    section.style.cssText = "align-items:center;text-align:center;padding-top:2rem";

    const logo = document.createElement("img");
    logo.src = "/zenith-icon.png";
    logo.alt = "Zenith";
    logo.style.cssText = "width:64px;height:64px;margin-bottom:0.5rem;border-radius:12px";
    section.append(logo);

    const name = document.createElement("div");
    name.className = "zen-section__title";
    name.textContent = "Zenith";
    section.append(name);

    const desc = document.createElement("p");
    desc.className = "zen-hint";
    desc.textContent = "A minimal top bar for Windows 11.";
    section.append(desc);

    let version = "0.1.0";
    try { version = await getVersion(); } catch { /* fallback */ }
    const verEl = document.createElement("p");
    verEl.className = "zen-hint";
    verEl.textContent = `v${version}`;
    section.append(verEl);

    const gh = document.createElement("a");
    gh.href = "https://github.com/b7s/zenith";
    gh.target = "_blank";
    gh.rel = "noopener";
    gh.className = "zen-link";
    gh.style.cssText = "margin-top:0.5rem";
    gh.textContent = "github.com/b7s/zenith";
    section.append(gh);

    pane.append(section);
  }

  async function buildGeneralTab(
    pane: HTMLElement,
    update: (patch: Partial<Config>) => void
  ) {
    const section = document.createElement("div");
    section.className = "zen-section";

    const row = document.createElement("label");
    row.className = "zen-checkbox";

    const text = document.createElement("span");
    text.className = "zen-checkbox__text";
    const lbl = document.createElement("span");
    lbl.className = "zen-checkbox__label";
    lbl.textContent = "Start with Windows";
    const desc = document.createElement("span");
    desc.className = "zen-checkbox__desc";
    desc.textContent = "Launch Zenith automatically when you sign in.";
    text.append(lbl, desc);
    row.append(text);

    const switchEl = document.createElement("span");
    switchEl.className = "zen-checkbox__switch";
    const input = document.createElement("input");
    input.type = "checkbox";
    switchEl.append(input);
    const track = document.createElement("span");
    track.className = "zen-checkbox__track";
    const thumb = document.createElement("span");
    thumb.className = "zen-checkbox__thumb";
    track.append(thumb);
    switchEl.append(track);
    row.append(switchEl);

    section.append(row);

    // --- Auto update ---
    const updRow = document.createElement("label");
    updRow.className = "zen-checkbox";

    const updText = document.createElement("span");
    updText.className = "zen-checkbox__text";
    const updLbl = document.createElement("span");
    updLbl.className = "zen-checkbox__label";
    updLbl.textContent = "Automatic update check";
    const updDesc = document.createElement("span");
    updDesc.className = "zen-checkbox__desc";
    updDesc.append("Checks for new versions every 12 hours. View releases at ");
    const relLink = document.createElement("a");
    relLink.href = "https://github.com/b7s/zenith/releases";
    relLink.target = "_blank";
    relLink.rel = "noopener";
    relLink.className = "zen-link";
    relLink.textContent = "GitHub releases";
    updDesc.append(relLink);
    updText.append(updLbl, updDesc);
    updRow.append(updText);

    const updSwitchEl = document.createElement("span");
    updSwitchEl.className = "zen-checkbox__switch";
    const updInput = document.createElement("input");
    updInput.type = "checkbox";
    updInput.checked = config.updates.auto_update;
    updSwitchEl.append(updInput);
    const updTrack = document.createElement("span");
    updTrack.className = "zen-checkbox__track";
    const updThumb = document.createElement("span");
    updThumb.className = "zen-checkbox__thumb";
    updTrack.append(updThumb);
    updSwitchEl.append(updTrack);
    updRow.append(updSwitchEl);

    section.append(updRow);

    // --- Status + check-now ---
    const statusRow = document.createElement("div");
    statusRow.style.cssText = "display:flex;align-items:center;justify-content:flex-end;gap:0.75rem;margin-top:0.5rem;flex-wrap:wrap";

    const statusHint = document.createElement("span");
    statusHint.className = "zen-update-status";
    statusHint.textContent = "Checking for updates…";

    const checkBtn = document.createElement("button");
    checkBtn.className = "zen-button zen-button--outline zen-button--sm";
    checkBtn.textContent = "Check now";
    statusRow.append(statusHint, checkBtn);
    section.append(statusRow);

    pane.append(section);

    try {
      const enabled = await invoke<boolean>(CMD.isStartWithWindows);
      input.checked = enabled;
    } catch { /* ignore */ }

    input.addEventListener("change", () => {
      void invoke(CMD.setStartWithWindows, { enabled: input.checked });
    });

    const setStatus = (s: UpdateStatus) => {
      statusHint.classList.remove("is-available", "is-error");
      if (s.update_available) {
        statusHint.textContent = `New version available: ${s.latest_version}`;
        statusHint.classList.add("is-available");
      } else if (s.message.startsWith("Check failed")) {
        statusHint.textContent = s.message;
        statusHint.classList.add("is-error");
      } else {
        statusHint.textContent = s.message || "Up to date";
      }
    };

    updInput.addEventListener("change", () => {
      update({ updates: { auto_update: updInput.checked } });
    });

    checkBtn.addEventListener("click", async () => {
      checkBtn.disabled = true;
      checkBtn.textContent = "Checking…";
      try {
        const s = await invoke<UpdateStatus>(CMD.checkUpdate);
        setStatus(s);
      } catch {
        statusHint.textContent = "Check failed";
      } finally {
        checkBtn.disabled = false;
        checkBtn.textContent = "Check now";
      }
    });

    try {
      const s = await invoke<UpdateStatus>(CMD.getUpdateStatus);
      setStatus(s);
    } catch { /* ignore */ }
  }
})();
