import { createElement, type IconNode } from "lucide";
import {
  Activity,
  Battery,
  BatteryCharging,
  BatteryFull,
  BatteryLow,
  BatteryMedium,
  BatteryWarning,
  ChevronDown,
  ChevronUp,
  Clock,
  CloudSun,
  ExternalLink,
  LayoutGrid,
  MonitorSmartphone,
  Music,
  Power,
  RotateCcw,
  Search,
  Settings,
  Volume,
  Volume1,
  Volume2,
  VolumeX,
  X,
} from "lucide";

import { DEFAULT_WIN_GLYPH, WIN_GLYPHS } from "./win-icons";

const OCTAGON_NODE: IconNode = [
  "svg",
  { xmlns: "http://www.w3.org/2000/svg", width: 24, height: 24, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": 2, "stroke-linecap": "round", "stroke-linejoin": "round" },
  [["polygon", { points: "20.3,15.4 15.4,20.3 8.6,20.3 3.7,15.4 3.7,8.6 8.6,3.7 15.4,3.7 20.3,8.6" }], ["circle", { cx: "12", cy: "12", r: "3" }]],
];

const ICON_REGISTRY: Record<string, IconNode> = {
  settings: Settings,
  "layout-grid": LayoutGrid,
  x: X,
  "rotate-ccw": RotateCcw,
  search: Search,
  power: Power,
  clock: Clock,
  battery: Battery,
  "battery-charging": BatteryCharging,
  "battery-full": BatteryFull,
  "battery-low": BatteryLow,
  "battery-medium": BatteryMedium,
  "battery-warning": BatteryWarning,
  "settings-octa": OCTAGON_NODE,
  volume: Volume,
  "volume-1": Volume1,
  "volume-2": Volume2,
  "volume-x": VolumeX,
  music: Music,
  activity: Activity,
  "cloud-sun": CloudSun,
  "monitor-smartphone": MonitorSmartphone,
  "external-link": ExternalLink,
  "chevron-up": ChevronUp,
  "chevron-down": ChevronDown,
};

const ALIASES: Record<string, string> = {
  close: "x",
  cancel: "x",
  restart: "rotate-ccw",
  refresh: "rotate-ccw",
  widgets: "layout-grid",
  volume: "volume",
  "now-playing": "music",
  "system-stats": "activity",
  weather: "cloud-sun",
  workspace: "monitor-smartphone",
  config: "settings-octa",
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

const SVG_NS = "http://www.w3.org/2000/svg";
const SPRITE_ID = "zen-icon-sprite";

function ensureSprite(): SVGElement {
  let sprite = document.getElementById(SPRITE_ID) as SVGElement | null;
  if (!sprite) {
    sprite = document.createElementNS(SVG_NS, "svg");
    sprite.id = SPRITE_ID;
    sprite.setAttribute("aria-hidden", "true");
    sprite.style.cssText = "position:absolute;width:0;height:0;overflow:hidden";
    document.documentElement.prepend(sprite);
  }
  return sprite;
}

function ensureSymbol(canonical: string, node: IconNode): string {
  const id = `zen-i-${canonical}`;
  const sprite = ensureSprite();
  if (sprite.querySelector(`#${id}`)) return id;

  const full = createElement(node);
  const symbol = document.createElementNS(SVG_NS, "symbol");
  symbol.id = id;

  for (const attr of full.getAttributeNames()) {
    if (attr === "id" || attr === "width" || attr === "height" || attr === "xmlns") continue;
    const val = full.getAttribute(attr);
    if (val !== null) symbol.setAttribute(attr, val);
  }

  symbol.innerHTML = full.innerHTML;
  sprite.appendChild(symbol);
  return id;
}

export function setIcon(el: HTMLElement, name: string, opts: IconOptions = {}): void {
  const size = opts.size ?? DEFAULT_ICON_SIZE;

  let container: HTMLElement;
  if (el.classList.contains("zen-icon-button")) {
    container = el.querySelector<HTMLElement>(".zen-icon") ?? (() => {
      const c = document.createElement("span");
      c.className = "zen-icon";
      c.style.setProperty("--zen-icon-size", `${size}px`);
      el.append(c);
      return c;
    })();
  } else {
    el.classList.add("zen-icon");
    el.style.setProperty("--zen-icon-size", `${size}px`);
    container = el;
  }

  const resolved = resolveIcon(name);
  if (resolved.kind === "svg") {
    const kebab = toKebab(name);
    const aliased = resolveAlias(kebab);
    const canonical = ICON_REGISTRY[aliased] ? aliased : kebab;
    const symbolId = ensureSymbol(canonical, resolved.node);

    const svgEl = document.createElementNS(SVG_NS, "svg");
    svgEl.setAttribute("width", String(size));
    svgEl.setAttribute("height", String(size));
    if (opts.strokeWidth !== undefined) {
      svgEl.setAttribute("stroke-width", String(opts.strokeWidth));
    }

    const useEl = document.createElementNS(SVG_NS, "use");
    useEl.setAttribute("href", `#${symbolId}`);
    svgEl.appendChild(useEl);

    container.classList.remove("zen-icon--font");
    container.replaceChildren(svgEl);
  } else {
    container.classList.add("zen-icon--font");
    container.textContent = resolved.glyph;
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
