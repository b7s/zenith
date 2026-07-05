import { invoke } from "@tauri-apps/api/core";
import { CMD } from "./ipc";
import type { WidgetManifest, WidgetSource, Config, WidgetZone } from "./types";

export async function getWidgets(): Promise<WidgetManifest[]> {
  try {
    return await invoke<WidgetManifest[]>(CMD.getWidgets);
  } catch {
    return [];
  }
}

export async function getWidgetSource(id: string): Promise<WidgetSource | null> {
  try {
    return await invoke<WidgetSource | null>(CMD.getWidgetSource, { id });
  } catch {
    return null;
  }
}

export async function getWorkspaceCount(): Promise<number> {
  try {
    const ws = await invoke<{ id: number; label: string }[]>(CMD.getWorkspaces);
    return ws.length;
  } catch {
    return 0;
  }
}

const injectedCss = new Set<string>();

function injectWidgetCss(id: string, css: string): void {
  if (!css || injectedCss.has(id)) return;
  injectedCss.add(id);
  const style = document.createElement("style");
  style.setAttribute("data-widget-css", id);
  style.textContent = css;
  document.head.appendChild(style);
}

export function renderWidget(
  container: HTMLElement,
  source: WidgetSource,
  id: string,
): void {
  injectWidgetCss(id, source.css);

  container.innerHTML = source.html;
  container.dataset.widget = id;

  if (source.js) {
    const script = document.createElement("script");
    script.textContent = source.js;
    container.appendChild(script);
  }
}

export async function layoutBar(barElement: HTMLElement, cfg: Config): Promise<void> {
  barElement.innerHTML = "";
  barElement.style.height = `${cfg.appearance.bar_height}px`;

  const zones = new Map<WidgetZone, HTMLDivElement>();
  for (const zone of ["left", "center", "right"] as WidgetZone[]) {
    const el = document.createElement("div");
    el.className = "bar-zone";
    el.dataset.barZone = zone;
    barElement.appendChild(el);
    zones.set(zone, el);
  }

  const manifests = await getWidgets();
  const manifestMap = new Map(manifests.map((m) => [m.id, m]));

  const workspaceCount = await getWorkspaceCount();

  for (const id of cfg.widgets.enabled) {
    const man = manifestMap.get(id);
    if (!man) continue;

    const zone = cfg.widgets.positions[id] ?? man.default_zone;
    const zoneEl = zones.get(zone);
    if (!zoneEl) continue;

    // Hide workspace widget when only 1 desktop exists
    if (id === "workspace" && workspaceCount <= 1) continue;

    const source = await getWidgetSource(id);
    if (!source) continue;

    const slot = document.createElement("div");
    slot.className = "widget-slot";
    slot.style.minWidth = `${man.min_width}px`;
    slot.dataset.widgetId = id;

    renderWidget(slot, source, id);
    zoneEl.appendChild(slot);
  }
}
