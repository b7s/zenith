import { createElement, type IconNode } from "lucide";
import {
  Activity,
  Battery,
  Clock,
  CloudSun,
  LayoutGrid,
  MonitorSmartphone,
  Music,
  Power,
  RotateCcw,
  Search,
  Settings,
  Volume2,
  X,
} from "lucide";

import { DEFAULT_WIN_GLYPH, WIN_GLYPHS } from "./win-icons";

const ICON_REGISTRY: Record<string, IconNode> = {
  settings: Settings,
  "layout-grid": LayoutGrid,
  x: X,
  "rotate-ccw": RotateCcw,
  search: Search,
  power: Power,
  clock: Clock,
  battery: Battery,
  "volume-2": Volume2,
  music: Music,
  activity: Activity,
  "cloud-sun": CloudSun,
  "monitor-smartphone": MonitorSmartphone,
};

const ALIASES: Record<string, string> = {
  close: "x",
  cancel: "x",
  restart: "rotate-ccw",
  refresh: "rotate-ccw",
  widgets: "layout-grid",
  volume: "volume-2",
  "now-playing": "music",
  "system-stats": "activity",
  weather: "cloud-sun",
  workspace: "monitor-smartphone",
};

export function registerIcons(map: Record<string, IconNode>): void {
  for (const [name, node] of Object.entries(map)) {
    ICON_REGISTRY[toKebab(name)] = node;
  }
}

function toKebab(input: string): string {
  return input
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/[\s_]+/g, "-")
    .toLowerCase();
}

function resolveAlias(name: string): string {
  return ALIASES[name] ?? name;
}

function winGlyph(kebab: string, aliased: string): string {
  return WIN_GLYPHS[aliased] ?? WIN_GLYPHS[kebab] ?? DEFAULT_WIN_GLYPH;
}

export type ResolvedIcon =
  | { kind: "svg"; node: IconNode }
  | { kind: "font"; glyph: string };

export function resolveIcon(name: string): ResolvedIcon {
  const kebab = toKebab(name);
  const aliased = resolveAlias(kebab);
  const node = ICON_REGISTRY[aliased] ?? ICON_REGISTRY[kebab];
  if (node) return { kind: "svg", node };
  return { kind: "font", glyph: winGlyph(kebab, aliased) };
}

export interface IconOptions {
  size?: number;
  strokeWidth?: number;
}

export const DEFAULT_ICON_SIZE = 16;

export function setIcon(el: HTMLElement, name: string, opts: IconOptions = {}): void {
  const size = opts.size ?? DEFAULT_ICON_SIZE;
  el.classList.add("zen-icon");
  el.style.setProperty("--zen-icon-size", `${size}px`);
  el.innerHTML = "";

  const resolved = resolveIcon(name);
  if (resolved.kind === "svg") {
    const svg = createElement(resolved.node);
    svg.setAttribute("width", String(size));
    svg.setAttribute("height", String(size));
    if (opts.strokeWidth !== undefined) {
      svg.setAttribute("stroke-width", String(opts.strokeWidth));
    }
    el.classList.remove("zen-icon--font");
    el.appendChild(svg);
  } else {
    el.classList.add("zen-icon--font");
    el.textContent = resolved.glyph;
  }
}

export function applyIcons(root: ParentNode = document): void {
  const targets = Array.from(root.querySelectorAll<HTMLElement>("[data-icon]"));
  for (const el of targets) {
    const name = el.dataset.icon ?? "";
    const sizeAttr = el.dataset.size;
    const size = sizeAttr ? Number(sizeAttr) : undefined;
    setIcon(el, name, { size: Number.isFinite(size) ? size : undefined });
  }
}
