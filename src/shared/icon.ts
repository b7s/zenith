import { createElement, type IconNode } from "lucide";
import {
  Activity,
  AlarmClock,
  Battery,
  BatteryCharging,
  BatteryFull,
  BatteryLow,
  BatteryMedium,
  BatteryWarning,
  Bluetooth,
  Calendar,
  CalendarSearch,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Check,
  Clock,
  Cloud,
  CloudDrizzle,
  CloudFog,
  CloudLightning,
  CloudMoon,
  CloudRain,
  CloudSnow,
  CloudSun,
  Cloudy,
  Copy,
  Droplets,
  ExternalLink,
  Eye,
  Focus,
  Gauge,
  Globe,
  Image,
  GitBranch,
  GitPullRequest,
  GitPullRequestDraft,
  Github,
  LayoutGrid,
  List,
  Loader2,
  Lock,
  LogOut,
  MonitorSmartphone,
  Moon,
  Music,
  Navigation,
  Plane,
  Pencil,
  Plus,
  Power,
  RefreshCw,
  Repeat,
  RotateCcw,
  Search,
  Settings,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Sparkles,
  Sun,
  SunMoon,
  Sunrise,
  Sunset,
  Thermometer,
  ToggleRight,
  Trash2,
  TriangleAlert,
  Umbrella,
  Upload,
  Volume,
  Volume1,
  Volume2,
  VolumeX,
  Waves,
  Wifi,
  Wind,
  X,
} from "lucide";

import { DEFAULT_WIN_GLYPH, WIN_GLYPHS } from "./win-icons";

const OCTAGON_NODE: IconNode = [
  "svg",
  { xmlns: "http://www.w3.org/2000/svg", width: 24, height: 24, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": 2, "stroke-linecap": "round", "stroke-linejoin": "round" },
  [["polygon", { points: "20.3,15.4 15.4,20.3 8.6,20.3 3.7,15.4 3.7,8.6 8.6,3.7 15.4,3.7 20.3,8.6" }], ["circle", { cx: "12", cy: "12", r: "3" }]],
];

const SOLID_PLAY: IconNode = [
  "svg",
  { xmlns: "http://www.w3.org/2000/svg", width: 24, height: 24, viewBox: "0 0 24 24" },
  [["path", { d: "M5 3l16 9-16 9V3z", fill: "currentColor" }]],
];

const SOLID_PAUSE: IconNode = [
  "svg",
  { xmlns: "http://www.w3.org/2000/svg", width: 24, height: 24, viewBox: "0 0 24 24" },
  [
    ["rect", { x: "5", y: "3", width: "5.5", height: "18", rx: "1.25", fill: "currentColor" }],
    ["rect", { x: "13.5", y: "3", width: "5.5", height: "18", rx: "1.25", fill: "currentColor" }],
  ],
];

const ICON_REGISTRY: Record<string, IconNode> = {
  settings: Settings,
  "layout-grid": LayoutGrid,
  lock: Lock,
  "log-out": LogOut,
  x: X,
  "rotate-ccw": RotateCcw,
  search: Search,
  power: Power,
  clock: Clock,
  "alarm-clock": AlarmClock,
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
  "cloud-moon": CloudMoon,
  cloud: Cloud,
  "cloud-rain": CloudRain,
  "cloud-drizzle": CloudDrizzle,
  "cloud-lightning": CloudLightning,
  "cloud-snow": CloudSnow,
  "cloud-fog": CloudFog,
  cloudy: Cloudy,
  sun: Sun,
  moon: Moon,
  wind: Wind,
  droplets: Droplets,
  eye: Eye,
  gauge: Gauge,
  sunrise: Sunrise,
  sunset: Sunset,
  thermometer: Thermometer,
  umbrella: Umbrella,
  navigation: Navigation,
  waves: Waves,
  "monitor-smartphone": MonitorSmartphone,
  "external-link": ExternalLink,
  globe: Globe,
  upload: Upload,
  image: Image,
  "chevron-up": ChevronUp,
  "chevron-down": ChevronDown,
  "chevron-left": ChevronLeft,
  "chevron-right": ChevronRight,
  calendar: Calendar,
  "calendar-search": CalendarSearch,
  "refresh-cw": RefreshCw,
  repeat: Repeat,
  "triangle-alert": TriangleAlert,
  wifi: Wifi,
  bluetooth: Bluetooth,
  focus: Focus,
  plane: Plane,
  "sun-moon": SunMoon,
  "sliders-horizontal": SlidersHorizontal,
  "toggle-right": ToggleRight,
  sparkles: Sparkles,
  plus: Plus,
  "trash-2": Trash2,
  pencil: Pencil,
  list: List,
  play: SOLID_PLAY,
  pause: SOLID_PAUSE,
  "skip-back": SkipBack,
  "skip-forward": SkipForward,
  "git-branch": GitBranch,
  "git-pull-request": GitPullRequest,
  "git-pull-request-draft": GitPullRequestDraft,
  github: Github,
  check: Check,
  copy: Copy,
  loader: Loader2,
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
  git: "git-branch",
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

/// Map an OpenWeatherMap condition code (weather[0].id) + day/night
/// (weather[0].icon suffix 'd'/'n') to a Lucide icon name. Falls back to
/// "cloud" when no specific mapping exists. Single home — the popup window
/// and the bar widget both use this via `window.__zenith_weatherIcon`.
export function weatherIcon(code: number, icon?: string): string {
  const isNight = icon?.endsWith("n") ?? false;
  // Group 2xx: Thunderstorm
  if (code >= 200 && code < 300) return "cloud-lightning";
  // Group 3xx: Drizzle
  if (code >= 300 && code < 400) return "cloud-drizzle";
  // Group 5xx: Rain
  if (code >= 500 && code < 600) return "cloud-rain";
  // Group 6xx: Snow
  if (code >= 600 && code < 700) return "cloud-snow";
  // Group 7xx: Atmosphere (mist, fog, haze, sand, dust, etc.)
  if (code >= 700 && code < 800) return "cloud-fog";
  // Group 800: Clear
  if (code === 800) return isNight ? "moon" : "sun";
  // Group 801-804: Clouds
  if (code > 800) return isNight ? "cloud-moon" : "cloud-sun";
  // Default
  return "cloud";
}
