import "../../../src/shared/color";
import "./eyedropper.css";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { initLog, logInfo } from "../../../src/shared/log";
import { loadConfig } from "../../../src/shared/config";
import { CMD } from "../../../src/shared/ipc";
import { EVENT } from "../../../src/shared/events";
import { formatColor, type ColorFormat, type RGBA } from "../../../src/shared/color";

void (async () => {
  await initLog();
  logInfo("eyedropper ready");

  const root = document.getElementById("root");
  if (!root) return;
  const win = getCurrentWindow();

  // ---- DOM: crosshair cursor (OS renders it) + a floating color preview
  // glass centered above the hot spot. Frozen per-monitor screenshots are
  // rendered as opaque images filling the overlay — a transparent fullscreen
  // window over hardware-composited video (MPO) makes the video render as
  // black, so we show a frozen frame instead of relying on transparency.
  type Frame = { x: number; y: number; w: number; h: number; png_data_url: string };
  const frameLayer = document.createElement("div");
  frameLayer.className = "cp-dropper-frames";
  root.append(frameLayer);

  // Place the frozen frames as opaque images. Convert each monitor's virtual-
  // screen rect (OS physical px) into window-local CSS px.
  try {
    const frames = await invoke<Frame[]>(CMD.getEyedropperFrames);
    const pos = await win.outerPosition();
    const dpr = window.devicePixelRatio || 1;
    const winX = pos.x as number;
    const winY = pos.y as number;
    for (const f of frames) {
      const img = document.createElement("img");
      img.className = "cp-dropper-frame";
      img.src = f.png_data_url;
      img.draggable = false;
      img.style.left = `${(f.x - winX) / dpr}px`;
      img.style.top = `${(f.y - winY) / dpr}px`;
      img.style.width = `${f.w / dpr}px`;
      img.style.height = `${f.h / dpr}px`;
      frameLayer.append(img);
    }
  } catch {
    /* no frames — fall back to transparent overlay */
  }
  const glass = document.createElement("div");
  glass.className = "cp-dropper-glass";

  const ring = document.createElement("div");
  ring.className = "cp-dropper-ring";

  const swatch = document.createElement("div");
  swatch.className = "cp-dropper-swatch";

  const readout = document.createElement("div");
  readout.className = "cp-dropper-readout";
  readout.textContent = "#000000";

  ring.append(swatch);
  glass.append(ring, readout);
  root.append(glass);

  // Glass is centered above the crosshair. These offsets are the glass
  // dimensions at rest (padding 10px + ring 56px + gap 6px + readout ~20px).
  const OX = -38;
  const OY = -112;

  let copyFormat: ColorFormat = "hex";
  // Deferred config load — doesn't block the first sample.
  loadConfig()
    .then((cfg) => {
      const wc = cfg.widgets?.config?.["color_picker"] as
        | Record<string, unknown>
        | undefined;
      if (wc?.copy_format) copyFormat = wc.copy_format as ColorFormat;
    })
    .catch(() => {});

  function moveTo(clientX: number, clientY: number) {
    glass.style.transform = `translate(${clientX + OX}px, ${clientY + OY}px)`;
  }

  let lastSample = performance.now();

  async function sample() {
    const now = performance.now();
    if (now - lastSample < 28) return;
    lastSample = now;
    try {
      const [ox, oy] = await invoke<[number, number]>(CMD.getCursorPosition);
      const px = await invoke<[number, number, number, number]>(
        CMD.eyedropperPixel,
        { x: ox, y: oy },
      );
      const rgba: RGBA = { r: px[0], g: px[1], b: px[2], a: px[3] / 255 };
      const hex = `#${px[0].toString(16).padStart(2, "0")}${px[1]
        .toString(16)
        .padStart(2, "0")}${px[2].toString(16).padStart(2, "0")}`;
      swatch.style.background = hex;
      readout.textContent = formatColor(rgba, copyFormat).toUpperCase();
    } catch {
      /* out of bounds — keep last */
    }
  }

  async function pick() {
    try {
      const [ox, oy] = await invoke<[number, number]>(CMD.getCursorPosition);
      // Read from the cached frozen frame so the picked color matches exactly
      // what the user sees in the overlay (the live desktop may have changed,
      // e.g. a playing video moved on).
      const px = await invoke<[number, number, number, number]>(
        CMD.eyedropperPixel,
        { x: ox, y: oy },
      );
      const rgba: RGBA = { r: px[0], g: px[1], b: px[2], a: px[3] / 255 };
      const text = formatColor(rgba, copyFormat);
      try {
        await navigator.clipboard.writeText(text);
      } catch {
        /* clipboard may be blocked */
      }
      try {
        await emit(EVENT.eyedropperPicked, {
          r: px[0], g: px[1], b: px[2], a: px[3], cancelled: false,
        });
      } catch {
        /* ignore */
      }
    } catch {
      /* ignore */
    }
    await finish();
  }

  async function cancel() {
    try {
      await emit(EVENT.eyedropperPicked, { cancelled: true });
    } catch {
      /* ignore */
    }
    await finish();
  }

  async function finish() {
    try { await invoke(CMD.endEyedropper); } catch { /* ignore */ }
    await win.close().catch(() => {});
  }

  // ---- Input handlers ----

  document.addEventListener("mousemove", (e) => {
    moveTo(e.clientX, e.clientY);
    void sample();
  });

  document.addEventListener("mousedown", (e) => {
    e.preventDefault();
    void pick();
  });

  // ESC: ensure the root has keyboard focus so keydown fires.
  root.setAttribute("tabindex", "-1");
  document.addEventListener("pointerdown", () => root.focus());
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      void cancel();
    }
  });

  win.onFocusChanged(({ payload: focused }) => {
    if (!focused) void cancel();
  });
  window.addEventListener("blur", () => void cancel());

  // Seed the glass at the current cursor position so it's immediately visible.
  // The cache is already warm from `open_eyedropper` — no re-capture needed.
  try {
    const [ox, oy] = await invoke<[number, number]>(CMD.getCursorPosition);
    const pos = await win.outerPosition();
    const localX = (ox - (pos.x as number)) / (window.devicePixelRatio || 1);
    const localY = (oy - (pos.y as number)) / (window.devicePixelRatio || 1);
    moveTo(localX, localY);
    void sample();
    root.focus();
  } catch {
    /* ignore */
  }
})();
