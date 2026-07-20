import "../../../src/styles/globals.css";
import "./color-picker.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { mountWindow, applyTheme } from "../../../src/shared/window";
import { applyIcons } from "../../../src/shared/icon";
import { initLog, logInfo } from "../../../src/shared/log";
import { CMD } from "../../../src/shared/ipc";
import { EVENT } from "../../../src/shared/events";
import { formatColor, parseColorAny, hsvToRgb, rgbToHsv, type ColorFormat, type RGBA } from "../../../src/shared/color";

void (async () => {
  await initLog();
  logInfo("color picker ready");
  await applyTheme();

  const { content } = await mountWindow({
    title: "Color Picker",
  });

  const win = getCurrentWindow();

  // ---- Color state (HSVA is the source of truth for the square/sliders) ----
  interface HSVA { h: number; s: number; v: number; a: number; }
  let hsva: HSVA = { h: 0, s: 0.8, v: 0.9, a: 1 };
  let format: ColorFormat = "hex";

  function rgba(): RGBA {
    const c = hsvToRgb(hsva);
    return { ...c, a: hsva.a };
  }
  function setFromRgba(r: RGBA) {
    const { h, s, v } = rgbToHsv(r);
    hsva = { h, s, v, a: r.a };
  }

  // ---- Layout ----
  const wrap = document.createElement("div");
  wrap.className = "cp-picker";

  const top = document.createElement("div");
  top.className = "cp-picker__top";

  // SV square
  const sv = document.createElement("div");
  sv.className = "cp-sv";
  sv.innerHTML =
    '<div class="cp-sv__sat"></div><div class="cp-sv__val"></div>';
  const svPointer = document.createElement("div");
  svPointer.className = "cp-sv__pointer";
  sv.append(svPointer);

  // Right column: hue + alpha sliders + current swatch
  const sliders = document.createElement("div");
  sliders.className = "cp-sliders";

  const hue = document.createElement("div");
  hue.className = "cp-hue";
  const huePointer = document.createElement("div");
  huePointer.className = "cp-hue__pointer";
  hue.append(huePointer);

  const alpha = document.createElement("div");
  alpha.className = "cp-alpha";
  const alphaPointer = document.createElement("div");
  alphaPointer.className = "cp-alpha__pointer";
  alpha.append(alphaPointer);

  const swatchRow = document.createElement("div");
  swatchRow.className = "cp-current";
  const bigSwatch = document.createElement("div");
  bigSwatch.className = "cp-current__swatch";
  const infoWrap = document.createElement("div");
  infoWrap.className = "cp-current__info";
  const readout = document.createElement("div");
  readout.className = "cp-current__readout";
  const badge = document.createElement("span");
  badge.className = "cp-format-badge";
  badge.textContent = "HEX";
  infoWrap.append(readout, badge);
  swatchRow.append(bigSwatch, infoWrap);

  sliders.append(hue, alpha, swatchRow);
  top.append(sv, sliders);

  // ---- Text input ----
  const input = document.createElement("input");
  input.type = "text";
  input.className = "zen-input cp-input";
  input.spellcheck = false;
  input.autocomplete = "off";

  // ---- Format radio cards ----
  const formats: ColorFormat[] = ["hex", "rgb", "hsl", "oklch", "hsv"];
  const fmtGroup = document.createElement("div");
  fmtGroup.className = "zen-radio-group cp-formats";
  const fmtCards: Record<string, HTMLLabelElement> = {};
  for (const f of formats) {
    const label = document.createElement("label");
    label.className = "zen-radio-card";
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "cp-format";
    radio.value = f;
    radio.checked = f === format;
    if (f === format) label.classList.add("is-selected");
    radio.addEventListener("change", () => {
      format = f;
      fmtGroup.querySelectorAll(".zen-radio-card").forEach((c) => c.classList.remove("is-selected"));
      label.classList.add("is-selected");
      refresh();
    });
    const span = document.createElement("span");
    span.textContent = f.toUpperCase();
    label.append(radio, span);
    fmtCards[f] = label;
    fmtGroup.append(label);
  }

  // ---- Swatches ----
  const swatchBar = document.createElement("div");
  swatchBar.className = "cp-swatches";
  const swatchColors = [
    "#ff0000", "#ff8000", "#ffff00", "#00ff00", "#00ffff", "#0000ff",
    "#8000ff", "#ff00ff", "#ffffff", "#9ca3af", "#000000", "#1e293b",
    "rgba(255,0,0,0.5)", "rgba(0,128,255,0.4)", "oklch(0.7 0.15 200)",
    "hsl(280 70% 50%)",
  ];
  for (const col of swatchColors) {
    const parsed = parseColorAny(col);
    if (!parsed) continue;
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = "cp-swatch";
    sw.style.background = col;
    sw.title = col;
    sw.addEventListener("click", () => {
      setFromRgba(parsed);
      refresh();
    });
    swatchBar.append(sw);
  }

  // ---- Action buttons ----
  const actions = document.createElement("div");
  actions.className = "cp-actions";

  const eyedropBtn = document.createElement("button");
  eyedropBtn.type = "button";
  eyedropBtn.className = "zen-button is-outline cp-act";
  eyedropBtn.append(createIcon("eyedropper", 16));
  eyedropBtn.append(document.createTextNode(" Sample"));
  eyedropBtn.title = "Pick a color from the screen";
  eyedropBtn.addEventListener("click", () => {
    invoke(CMD.openEyedropper).catch(() => {});
  });

  const copyBtn = document.createElement("button");
  copyBtn.type = "button";
  copyBtn.className = "zen-button is-primary cp-act";
  copyBtn.append(createIcon("copy", 16));
  copyBtn.append(document.createTextNode(" Copy"));
  copyBtn.title = "Copy in the selected format";
  copyBtn.addEventListener("click", () => {
    copyCurrent();
  });

  actions.append(eyedropBtn, copyBtn);

  wrap.append(top, input, fmtGroup, swatchBar, actions);
  content.append(wrap);
  applyIcons(content);

  // ---- Drag helpers ----
  function makeDrag(el: HTMLElement, onMove: (fx: number, fy: number) => void) {
    let dragging = false;
    const compute = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const fx = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
      const fy = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
      onMove(fx, fy);
    };
    el.addEventListener("pointerdown", (e) => {
      dragging = true;
      el.setPointerCapture(e.pointerId);
      compute(e);
    });
    el.addEventListener("pointermove", (e) => {
      if (dragging) compute(e);
    });
    const end = (e: PointerEvent) => {
      dragging = false;
      try { el.releasePointerCapture(e.pointerId); } catch { /* ignore */ }
    };
    el.addEventListener("pointerup", end);
    el.addEventListener("pointercancel", end);
  }

  makeDrag(sv, (fx, fy) => {
    hsva.s = fx;
    hsva.v = 1 - fy;
    refresh();
  });
  makeDrag(hue, (fx) => {
    hsva.h = fx * 360;
    refresh();
  });
  makeDrag(alpha, (fx) => {
    hsva.a = fx;
    refresh();
  });

  input.addEventListener("change", () => {
    const parsed = parseColorAny(input.value);
    if (parsed) {
      setFromRgba(parsed);
      refresh();
    } else {
      flashReadout("Invalid color");
    }
  });

  // Listen for eyedropper pick (from this window's own Sample button or the
  // bar widget) and adopt the color. The listener is scoped to this webview
  // and cleaned up automatically when the window closes.
  await listen<{ r?: number; g?: number; b?: number; a?: number; cancelled?: boolean }>(
    EVENT.eyedropperPicked,
    (ev) => {
      const p = ev.payload;
      if (p.cancelled || p.r === undefined) return;
      setFromRgba({ r: p.r, g: p.g ?? 0, b: p.b ?? 0, a: (p.a ?? 255) / 255 });
      refresh();
    },
  );

  // ---- Rendering ----
  function hslCss(h: number, s: number, l: number) {
    return `hsl(${h} ${s * 100}% ${l * 100}%)`;
  }

  function refresh() {
    const { h, s, v, a } = hsva;
    const r = rgba();

    // SV square backgrounds
    const baseHue = hslCss(h, 1, 0.5);
    (sv.querySelector(".cp-sv__sat") as HTMLElement).style.background =
      `linear-gradient(to right, #fff, ${baseHue})`;
    (sv.querySelector(".cp-sv__val") as HTMLElement).style.background =
      "linear-gradient(to top, #000, transparent)";
    // pointer
    svPointer.style.left = `${s * 100}%`;
    svPointer.style.top = `${(1 - v) * 100}%`;
    const ptrColor = formatColor(r, "hex");
    svPointer.style.background = ptrColor;
    svPointer.style.boxShadow =
      a < 1
        ? `0 0 0 2px #fff, 0 0 0 3px rgba(0,0,0,.5)`
        : `0 0 0 2px #fff, 0 0 0 3px rgba(0,0,0,.5)`;

    // Hue pointer
    huePointer.style.left = `${(h / 360) * 100}%`;

    // Alpha gradient + pointer
    const solid = formatColor({ ...r, a: 1 }, "hex");
    alpha.style.background =
      `linear-gradient(to right, rgba(0,0,0,0), ${solid}), ` +
      `repeating-conic-gradient(#ccc 0% 25%, #fff 0% 50%) 0 / 14px 14px`;
    alphaPointer.style.left = `${a * 100}%`;

    // Current swatch + readout
    bigSwatch.style.background = a < 1
      ? `linear-gradient(${solid}, ${solid}), repeating-conic-gradient(#ccc 0% 25%, #fff 0% 50%) 0 / 12px 12px`
      : solid;
    readout.textContent = formatColor(r, format);
    badge.textContent = format.toUpperCase();

    // Keep the input in sync only when it's not focused (avoid clobbering
    // the user mid-edit).
    if (document.activeElement !== input) {
      input.value = formatColor(r, format);
    }

    fit();
  }

  let flashTimer: number | null = null;
  function flashReadout(msg: string) {
    readout.textContent = msg;
    if (flashTimer !== null) clearTimeout(flashTimer);
    flashTimer = window.setTimeout(() => refresh(), 1200);
  }

  async function copyCurrent() {
    const text = formatColor(rgba(), format);
    try {
      await navigator.clipboard.writeText(text);
      flashReadout(`Copied ${text}`);
    } catch {
      flashReadout("Copy failed");
    }
  }

  function fit() {
    requestAnimationFrame(() => {
      const h = Math.min(640, Math.max(520, document.body.scrollHeight));
      win.setSize(new LogicalSize(360, h)).catch(() => {});
    });
  }

  // Close on Escape. Dragging the header is handled by mountWindow's
  // enableDrag — don't close on focus loss or the drag itself trips it.
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") win.close().catch(() => {});
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
      e.preventDefault();
      void copyCurrent();
    }
  });

  // Initial paint.
  refresh();
  // Ensure keyboard focus so ESC keydown fires.
  content.setAttribute("tabindex", "-1");
  content.focus();
})();

function createIcon(name: string, size: number): HTMLElement {
  const i = document.createElement("i");
  i.dataset.icon = name;
  i.dataset.size = String(size);
  return i;
}
