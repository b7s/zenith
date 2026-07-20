/**
 * Pure color conversions — single home for hex / rgb / hsv / hsl / oklch.
 * No DOM, no IPC, no side effects. Used by the color-picker widget + window.
 *
 * Conventions:
 *   - RGB channels are 0..255 integers.
 *   - HSV / HSL hue is 0..360 degrees; saturation/value/lightness are 0..1.
 *   - OKLCH L is 0..1, C is 0..~0.4 (unbounded but typical), H is 0..360.
 *   - Alpha is always 0..1 (1 = opaque).
 *
 * Mirrors no Rust struct — the color-picker domain is frontend-only.
 */

export type ColorFormat = "hex" | "rgb" | "hsv" | "hsl" | "oklch";

export interface RGBA {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface HSVA {
  h: number;
  s: number;
  v: number;
  a: number;
}

export interface HSLA {
  h: number;
  s: number;
  l: number;
  a: number;
}

export interface OKLCHA {
  l: number;
  c: number;
  h: number;
  a: number;
}

const clamp = (n: number, lo: number, hi: number): number =>
  Math.min(hi, Math.max(lo, n));

const clamp255 = (n: number): number => Math.round(clamp(n, 0, 255));
const clamp01 = (n: number): number => clamp(n, 0, 1);

export function normalizeHex(input: string): string {
  let s = input.trim().replace(/^#/, "").toLowerCase();
  if (s.length === 3 || s.length === 4) {
    s = s
      .split("")
      .map((c) => c + c)
      .join("");
  }
  if (s.length !== 6 && s.length !== 8) return "";
  if (!/^[0-9a-f]+$/.test(s)) return "";
  return "#" + s;
}

export function parseHex(hex: string): RGBA | null {
  const n = normalizeHex(hex);
  if (!n) return null;
  const r = parseInt(n.slice(1, 3), 16);
  const g = parseInt(n.slice(3, 5), 16);
  const b = parseInt(n.slice(5, 7), 16);
  const a = n.length === 9 ? parseInt(n.slice(7, 9), 16) / 255 : 1;
  return { r, g, b, a };
}

export function toHex({ r, g, b, a }: RGBA): string {
  const h = (n: number) => clamp255(n).toString(16).padStart(2, "0");
  const base = `#${h(r)}${h(g)}${h(b)}`;
  return a >= 1 ? base : `${base}${h(Math.round(a * 255))}`;
}

export function rgbToHsv({ r, g, b }: RGBA): { h: number; s: number; v: number } {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === rn) h = ((gn - bn) / d) % 6;
    else if (max === gn) h = (bn - rn) / d + 2;
    else h = (rn - gn) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  const s = max === 0 ? 0 : d / max;
  return { h, s, v: max };
}

export function hsvToRgb({ h, s, v }: { h: number; s: number; v: number }): RGBA {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  let rp = 0;
  let gp = 0;
  let bp = 0;
  if (h < 60) [rp, gp, bp] = [c, x, 0];
  else if (h < 120) [rp, gp, bp] = [x, c, 0];
  else if (h < 180) [rp, gp, bp] = [0, c, x];
  else if (h < 240) [rp, gp, bp] = [0, x, c];
  else if (h < 300) [rp, gp, bp] = [x, 0, c];
  else [rp, gp, bp] = [c, 0, x];
  return {
    r: Math.round((rp + m) * 255),
    g: Math.round((gp + m) * 255),
    b: Math.round((bp + m) * 255),
    a: 1,
  };
}

export function rgbToHsl({ r, g, b }: RGBA): { h: number; s: number; l: number } {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  const d = max - min;
  let h = 0;
  let s = 0;
  if (d !== 0) {
    s = d / (1 - Math.abs(2 * l - 1));
    if (max === rn) h = ((gn - bn) / d) % 6;
    else if (max === gn) h = (bn - rn) / d + 2;
    else h = (rn - gn) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s, l };
}

export function hslToRgb({ h, s, l }: { h: number; s: number; l: number }): RGBA {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  let rp = 0;
  let gp = 0;
  let bp = 0;
  if (h < 60) [rp, gp, bp] = [c, x, 0];
  else if (h < 120) [rp, gp, bp] = [x, c, 0];
  else if (h < 180) [rp, gp, bp] = [0, c, x];
  else if (h < 240) [rp, gp, bp] = [0, x, c];
  else if (h < 300) [rp, gp, bp] = [x, 0, c];
  else [rp, gp, bp] = [c, 0, x];
  return {
    r: Math.round((rp + m) * 255),
    g: Math.round((gp + m) * 255),
    b: Math.round((bp + m) * 255),
    a: 1,
  };
}

// sRGB → linear → OKLab → OKLCH (W3C CSS Color 4 reference implementation).
function srgbToLinear(c: number): number {
  return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}
function linearToSrgb(c: number): number {
  return c <= 0.0031308 ? c * 12.92 : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
}

export function rgbToOklch({ r, g, b }: RGBA): { l: number; c: number; h: number } {
  const lr = srgbToLinear(r / 255);
  const lg = srgbToLinear(g / 255);
  const lb = srgbToLinear(b / 255);

  const l = 0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb;
  const m = 0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb;
  const s = 0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb;

  const l_ = Math.cbrt(l);
  const m_ = Math.cbrt(m);
  const s_ = Math.cbrt(s);

  const L = 0.2104542553 * l_ + 0.793617785 * m_ - 0.0040720468 * s_;
  const a = 1.9779984951 * l_ - 2.428592205 * m_ + 0.4505937099 * s_;
  const bb = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.808675766 * s_;

  const C = Math.sqrt(a * a + bb * bb);
  let H = (Math.atan2(bb, a) * 180) / Math.PI;
  if (H < 0) H += 360;
  return { l: L, c: C, h: H };
}

export function oklchToRgb({ l, c, h }: { l: number; c: number; h: number }): RGBA {
  const hr = (h * Math.PI) / 180;
  const a = c * Math.cos(hr);
  const bb = c * Math.sin(hr);

  const l_ = l + 0.3963377774 * a + 0.2158037573 * bb;
  const m_ = l - 0.1055613458 * a - 0.0638541728 * bb;
  const s_ = l - 0.0894841775 * a - 1.291485548 * bb;

  const lv = l_ * l_ * l_;
  const mv = m_ * m_ * m_;
  const sv = s_ * s_ * s_;

  const rL = 4.0767416621 * lv - 3.3077115913 * mv + 0.2309699292 * sv;
  const gL = -1.2684380046 * lv + 2.6097574011 * mv - 0.3413193965 * sv;
  const bL = -0.0041960863 * lv - 0.7034186147 * mv + 1.707614701 * sv;

  const r = linearToSrgb(rL);
  const g = linearToSrgb(gL);
  const b = linearToSrgb(bL);

  return {
    r: Math.round(clamp(r, 0, 1) * 255),
    g: Math.round(clamp(g, 0, 1) * 255),
    b: Math.round(clamp(b, 0, 1) * 255),
    a: 1,
  };
}

/** Render an RGBA in the chosen format. Alpha is omitted when fully opaque. */
export function formatColor(rgba: RGBA, fmt: ColorFormat): string {
  const a = clamp01(rgba.a);
  switch (fmt) {
    case "hex":
      return toHex({ ...rgba, a });
    case "rgb":
      return a < 1
        ? `rgb(${rgba.r} ${rgba.g} ${rgba.b} / ${trim(a)})`
        : `rgb(${rgba.r} ${rgba.g} ${rgba.b})`;
    case "hsv": {
      const { h, s, v } = rgbToHsv(rgba);
      return a < 1
        ? `hsv(${Math.round(h)} ${pct(s)} ${pct(v)} / ${trim(a)})`
        : `hsv(${Math.round(h)} ${pct(s)} ${pct(v)})`;
    }
    case "hsl": {
      const { h, s, l } = rgbToHsl(rgba);
      return a < 1
        ? `hsl(${Math.round(h)} ${pct(s)} ${pct(l)} / ${trim(a)})`
        : `hsl(${Math.round(h)} ${pct(s)} ${pct(l)})`;
    }
    case "oklch": {
      const { l, c, h } = rgbToOklch(rgba);
      return a < 1
        ? `oklch(${pct(l)} ${trim(c)} ${Math.round(h)} / ${trim(a)})`
        : `oklch(${pct(l)} ${trim(c)} ${Math.round(h)})`;
    }
  }
}

/** Best-effort parse of any CSS-style color string into RGBA. Returns null on failure. */
export function parseColorAny(input: string): RGBA | null {
  const s = input.trim();
  if (!s) return null;
  if (s.startsWith("#")) return parseHex(s);

  const m = s.match(/^rgba?\(([^)]+)\)$/i);
  if (m) {
    const parts = m[1].split(/[/,\s]+/).filter(Boolean).map((p) => p.trim());
    const nums = parts.map((p) => parseFloat(p));
    if (nums.length >= 3) {
      return {
        r: clamp255(nums[0]),
        g: clamp255(nums[1]),
        b: clamp255(nums[2]),
        a: nums[3] === undefined ? 1 : clamp01(nums[3] > 1 ? nums[3] / 255 : nums[3]),
      };
    }
    return null;
  }

  const mh = s.match(/^hsla?\(([^)]+)\)$/i);
  if (mh) {
    const parts = mh[1].split(/[/,\s]+/).filter(Boolean).map((p) => p.trim());
    if (parts.length >= 3) {
      const h = parseFloat(parts[0]);
      const sl = parseFloat(parts[1]) / 100;
      const lOrV = parseFloat(parts[2]) / 100;
      const a = parts[3] === undefined ? 1 : clamp01(parseFloat(parts[3]));
      if (s.toLowerCase().startsWith("hsv")) {
        return { ...hsvToRgb({ h, s: sl, v: lOrV }), a };
      }
      return { ...hslToRgb({ h, s: sl, l: lOrV }), a };
    }
    return null;
  }

  const mo = s.match(/^oklch\(([^)]+)\)$/i);
  if (mo) {
    const parts = mo[1].split(/[/,\s]+/).filter(Boolean).map((p) => p.trim());
    if (parts.length >= 3) {
      const l = parseFloat(parts[0]) / (parts[0].endsWith("%") ? 100 : 1);
      const c = parseFloat(parts[1]);
      const h = parseFloat(parts[2]);
      const a = parts[3] === undefined ? 1 : clamp01(parseFloat(parts[3]));
      return { ...oklchToRgb({ l, c, h }), a };
    }
    return null;
  }

  return null;
}

function trim(n: number): string {
  return Number(n.toFixed(3)).toString();
}
function pct(n: number): string {
  return `${Math.round(n * 100)}%`;
}
