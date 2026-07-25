/**
 * Shared SVG chart helpers — used by the ai-cli usage charts and the weather
 * temperature chart. Per AGENTS §6.2, one concern lives in one place: all
 * axis drawing, axis scaling, peak lines, and tooltip overlay logic is here.
 *
 * Reference consumers: `widgets/ai_cli/window/main.ts` (bar + line charts),
 * `widgets/weather/window/main.ts` (7-day temperature line chart).
 *
 * See AGENTS §6.1b "SVG chart pattern" for the mandatory DOM + class contract.
 */

// ════════════════════════════════════════════════════════════════════
// Axis scaling
// ════════════════════════════════════════════════════════════════════

/**
 * Round `v` up to the next "nice" axis max. Uses a fine-grained step set so
 * the chart doesn't end up with a large empty top half (e.g. real max 301K
 * → axis 350K, not 500K).
 */
export function niceMax(v: number): number {
  if (v <= 0) return 1;
  const exp = Math.floor(Math.log10(v));
  const base = Math.pow(10, exp);
  const f = v / base;
  const steps = [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 7.5, 10];
  const nf = steps.find((s) => s >= f) ?? 10;
  return nf * base;
}

/**
 * Headroom-adjusted axis max: real peak × 1.1, then rounded up to a nice
 * scale so the peak bar/line doesn't touch the ceiling but the top of the
 * chart isn't excessively empty either.
 */
export function axisMax(peak: number): number {
  if (peak <= 0) return 1;
  return niceMax(peak * 1.1);
}

// ════════════════════════════════════════════════════════════════════
// SVG element helpers
// ════════════════════════════════════════════════════════════════════

const SVG_NS = "http://www.w3.org/2000/svg";

function svgText(parent: Element, text: string, x: number, y: number, anchor: "start" | "middle" | "end" = "start", cls = "zen-chart__axis-text"): void {
  const t = document.createElementNS(SVG_NS, "text");
  t.setAttribute("x", String(x));
  t.setAttribute("y", String(y));
  t.setAttribute("text-anchor", anchor);
  t.classList.add(cls);
  t.textContent = text;
  parent.append(t);
}

// ════════════════════════════════════════════════════════════════════
// Y-axis: value labels left of the plot area + horizontal gridlines
// ════════════════════════════════════════════════════════════════════

export function drawYAxis(
  svgEl: Element,
  yMax: number,
  yFmt: (n: number) => string,
  PAD_L: number, PAD_T: number, plotW: number, plotH: number,
): void {
  if (yMax <= 0) yMax = 1;
  const steps = 4;
  for (let i = 0; i <= steps; i++) {
    const val = (yMax * i) / steps;
    const y = PAD_T + plotH - (val / yMax) * plotH;
    const ln = document.createElementNS(SVG_NS, "line");
    ln.setAttribute("x1", String(PAD_L));
    ln.setAttribute("y1", String(y));
    ln.setAttribute("x2", String(PAD_L + plotW));
    ln.setAttribute("y2", String(y));
    ln.classList.add("zen-chart__grid");
    svgEl.append(ln);
    svgText(svgEl, yFmt(val), PAD_L - 4, y + 3, "end");
  }
}

// ════════════════════════════════════════════════════════════════════
// X-axis: day labels under the plot area (first, middle, last + a few intervals)
// ════════════════════════════════════════════════════════════════════

export function drawXAxis(
  svgEl: Element,
  days: string[],
  dayFmt: (day: string) => string,
  PAD_L: number, PAD_T: number, plotW: number, plotH: number,
): void {
  const n = days.length;
  const botY = PAD_T + plotH;
  const xs = (i: number): number => {
    if (n === 1) return PAD_L + plotW / 2;
    return PAD_L + (i * plotW) / (n - 1);
  };
  const indices = new Set<number>([0, n - 1]);
  if (n > 4) {
    indices.add(Math.floor(n / 2));
    indices.add(Math.floor(n / 4));
    indices.add(Math.floor((3 * n) / 4));
  } else if (n > 2) {
    indices.add(Math.floor(n / 2));
  }
  for (const idx of Array.from(indices).sort((a, b) => a - b)) {
    svgText(svgEl, dayFmt(days[idx]), xs(idx), botY + 14, "middle");
  }
}

// ════════════════════════════════════════════════════════════════════
// Peak dashed line with a label
// ════════════════════════════════════════════════════════════════════

export function drawPeakLine(
  svgEl: Element,
  peakVal: number,
  yScale: (v: number) => number,
  label: string,
  PAD_L: number, plotW: number,
): void {
  const y = yScale(peakVal);
  const ln = document.createElementNS(SVG_NS, "line");
  ln.setAttribute("x1", String(PAD_L));
  ln.setAttribute("y1", String(y));
  ln.setAttribute("x2", String(PAD_L + plotW));
  ln.setAttribute("y2", String(y));
  ln.classList.add("zen-chart__peak");
  svgEl.append(ln);
  svgText(svgEl, label, PAD_L + plotW - 2, y - 3, "end", "zen-chart__peak-label");
}

// ════════════════════════════════════════════════════════════════════
// Chart tooltip — HTML overlay positioned in the chart wrap (mirrors the
// weather chart pattern: a transparent hit target on each data point +
// `mouseenter`/`mousemove`/`mouseleave` to show/move/hide an HTML div).
// ════════════════════════════════════════════════════════════════════

let _globalTooltip: HTMLElement | null = null;

export function makeTooltip(_wrap: HTMLElement): HTMLElement {
  if (!_globalTooltip) {
    _globalTooltip = document.createElement("div");
    _globalTooltip.className = "zen-chart__tooltip";
    document.body.append(_globalTooltip);
  }
  return _globalTooltip;
}

function moveTooltip(e: MouseEvent, tooltip: HTMLElement): void {
  const ww = window.innerWidth, wh = window.innerHeight;
  const tw = tooltip.offsetWidth, th = tooltip.offsetHeight;
  let x = e.clientX + 10;
  let y = e.clientY - 10;
  if (x + tw > ww) x = e.clientX - tw - 10;
  if (y < 0) y = e.clientY + 10;
  if (y + th > wh) y = e.clientY - th - 10;
  tooltip.style.left = x + "px";
  tooltip.style.top = y + "px";
}

export function showTooltip(e: MouseEvent, tooltip: HTMLElement, html: string): void {
  tooltip.innerHTML = html;
  tooltip.style.opacity = "1";
  moveTooltip(e, tooltip);
}

export function hideTooltip(tooltip: HTMLElement): void {
  tooltip.style.opacity = "0";
}

/**
 * Attach mouse listeners to a chart element (the hit-target circle/rect)
 * so it shows `html` in `tooltip` on hover.
 */
export function attachTooltip(el: SVGElement, tooltip: HTMLElement, html: string): void {
  el.addEventListener("mouseenter", (e) => showTooltip(e, tooltip, html));
  el.addEventListener("mousemove", (e) => moveTooltip(e, tooltip));
  el.addEventListener("mouseleave", () => hideTooltip(tooltip));
}

/** Make a transparent hit circle for tooltip triggering (large radius so
 *  the tooltip is easy to trigger — mirrors weather chart's `r=12`). */
export function makeHitDot(cx: number, cy: number, r = 12): SVGCircleElement {
  const hit = document.createElementNS(SVG_NS, "circle");
  hit.setAttribute("cx", String(cx));
  hit.setAttribute("cy", String(cy));
  hit.setAttribute("r", String(r));
  hit.classList.add("zen-chart__hit");
  return hit;
}
