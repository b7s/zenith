import "../../../src/styles/globals.css";
import "./weather.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setIcon, applyIcons, weatherIcon } from "../../../src/shared/icon";
import { mountWindow } from "../../../src/shared/window";
import { applyTheme } from "../../../src/shared/window";
import { initLog, logInfo } from "../../../src/shared/log";

interface WeatherSnapshot {
  ok: boolean;
  city?: string;
  units?: string;
  daily?: any[];
  current?: any;
  air?: any;
  error?: string;
  updated_at: number;
}

void (async () => {
  await initLog();
  logInfo("weather popup ready");

  const win = getCurrentWindow();
  const { root, content } = await mountWindow({ title: "Weather" });
  root.classList.add("weather-window");
  await applyTheme();

  let snap: WeatherSnapshot | null = null;
  try {
    snap = await invoke<WeatherSnapshot>("weather_get_cache");
  } catch {
    snap = null;
  }

  if (!snap || !snap.ok || !snap.current) {
    renderError(content, snap?.error || "Weather data not available. Configure the widget.");
    return;
  }

  render(content, snap);

  win.onFocusChanged(({ payload }) => {
    if (!payload) win.close().catch(() => {});
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") win.close().catch(() => {});
  });
})();

function renderError(content: HTMLElement, msg: string): void {
  content.className = "zen-window__content weather-main weather-error";
  content.innerHTML = `
    <div class="weather-error">
      <span class="zen-icon weather-error__icon" data-icon="triangle-alert" data-size="48"></span>
      <p class="weather-error__msg">${escapeHtml(msg)}</p>
      <p class="weather-error__hint">Open the widget settings to add a city and API key.</p>
    </div>
  `;
  applyIcons(content);
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => {
    const map: Record<string, string> = {
      "&": "&",
      "<": "<",
      ">": ">",
      '"': '"',
      "'": "'",
    };
    return map[c] || c;
  });
}

function render(content: HTMLElement, snap: WeatherSnapshot): void {
  content.className = "zen-window__content weather-main";
  content.innerHTML = "";

  const cur = snap.current!;
  const daily = snap.daily || [];
  const air = snap.air;
  const city = snap.city || "";
  const units = snap.units || "metric";
  const isImperial = units === "imperial";
  const unit = isImperial ? "\u00B0F" : "\u00B0C";

  // Current conditions card
  const currentCard = document.createElement("div");
  currentCard.className = "zen-card weather-current";
  currentCard.style.cssText = "margin-bottom:1rem;padding:1rem;";

  const code = cur.weather?.[0]?.id;
  const iconCode = cur.weather?.[0]?.icon;
  const iconName = weatherIcon(code, iconCode);

  const iconWrap = document.createElement("div");
  iconWrap.className = "weather-current__icon";
  const iconEl = document.createElement("span");
  iconEl.className = "zen-icon";
  iconEl.dataset.icon = iconName;
  iconEl.dataset.size = "48";
  setIcon(iconEl, iconName, { size: 48 });
  iconWrap.append(iconEl);

  const info = document.createElement("div");
  info.className = "weather-current__info";
  info.style.cssText = "flex:1;min-width:0;";

  const cityEl = document.createElement("div");
  cityEl.className = "weather-current__city";
  cityEl.style.cssText = "font-weight:600;font-size:1rem;color:var(--foreground);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;";
  cityEl.textContent = city;

  const tempEl = document.createElement("div");
  tempEl.className = "weather-current__temp";
  tempEl.style.cssText = "font-size:2.5rem;font-weight:700;font-variant-numeric:tabular-nums;line-height:1.1;color:var(--foreground);margin-top:0.25rem;";
  const t = cur.temp !== undefined && cur.temp !== null ? Math.round(cur.temp) : "--";
  tempEl.textContent = t + unit;

  // Today's high/low from the first daily forecast row
  const todayHi = daily[0]?.temp?.max;
  const todayLo = daily[0]?.temp?.min;
  let hiLoEl: HTMLElement | null = null;
  if (todayHi !== undefined && todayHi !== null && todayLo !== undefined && todayLo !== null) {
    hiLoEl = document.createElement("div");
    hiLoEl.className = "weather-current__hilo";
    hiLoEl.style.cssText = "display:flex;gap:0.75rem;align-items:center;font-variant-numeric:tabular-nums;font-size:0.85rem;color:var(--muted-foreground);margin-top:0.15rem;";

    const hi = document.createElement("span");
    hi.style.cssText = "display:inline-flex;align-items:center;gap:0.2rem;";
    const hiIcon = document.createElement("span");
    hiIcon.className = "zen-icon";
    hiIcon.dataset.icon = "arrow-up";
    hiIcon.dataset.size = "12";
    setIcon(hiIcon, "arrow-up", { size: 12 });
    hi.append(hiIcon, document.createTextNode(Math.round(todayHi) + unit));

    const lo = document.createElement("span");
    lo.style.cssText = "display:inline-flex;align-items:center;gap:0.2rem;";
    const loIcon = document.createElement("span");
    loIcon.className = "zen-icon";
    loIcon.dataset.icon = "arrow-down";
    loIcon.dataset.size = "12";
    setIcon(loIcon, "arrow-down", { size: 12 });
    lo.append(loIcon, document.createTextNode(Math.round(todayLo) + unit));

    hiLoEl.append(hi, lo);
  }

  const metaEl = document.createElement("div");
  metaEl.className = "weather-current__meta";
  metaEl.style.cssText = "font-size:0.75rem;color:var(--muted-foreground);display:flex;flex-wrap:wrap;gap:0.5rem;margin-top:0.25rem;";
  const feels = cur.feels_like !== undefined ? "Feels " + Math.round(cur.feels_like) + unit : "";
  const hum = cur.humidity !== undefined ? "Humidity " + cur.humidity + "%" : "";
  const wind = cur.wind_speed !== undefined ? "Wind " + Math.round(cur.wind_speed) + (isImperial ? " mph" : " m/s") : "";
  const desc = cur.weather?.[0]?.description || "";
  metaEl.textContent = [desc, feels, hum, wind].filter(Boolean).join(" \u00B7 ");

  info.append(cityEl, tempEl);
  if (hiLoEl) info.append(hiLoEl);
  info.append(metaEl);
  currentCard.append(iconWrap, info);
  content.append(currentCard);

  // 7-day chart
  if (daily.length > 0) {
    content.append(buildChart(daily));
  }

  // Metrics grid
  if (cur || air) {
    content.append(buildMetricsGrid(cur, air, daily, isImperial));
  }
}

function buildChart(daily: any[]): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "zen-card weather-chart";
  wrap.style.cssText = "margin-bottom:1rem;padding:1rem;position:relative;";

  const title = document.createElement("div");
  title.className = "weather-chart__title";
  title.style.cssText = "font-weight:600;margin-bottom:0.5rem;color:var(--foreground);";
  title.textContent = "7-Day Temperature";
  wrap.append(title);

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "weather-chart__svg");
  svg.setAttribute("viewBox", "0 0 360 200");
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", "7-day temperature chart");

  const w = 360, h = 200;
  const padL = 48, padR = 12, padT = 12, padB = 40;
  const plotW = w - padL - padR;
  const plotH = h - padT - padB;

  const maxs = daily.map((d) => d.temp?.max ?? null);
  const mins = daily.map((d) => d.temp?.min ?? null);
  const vals = [...maxs, ...mins].filter((v): v is number => v !== null);
  const minT = Math.min(...vals) - 2;
  const maxT = Math.max(...vals) + 2;
  const range = maxT - minT || 1;
  const xStep = daily.length > 1 ? plotW / (daily.length - 1) : 0;

  const y = (val: number) => padT + plotH - ((val - minT) / range) * plotH;
  const x = (i: number) => padL + i * xStep;

  // Tooltip div
  const tooltip = document.createElement("div");
  tooltip.style.cssText = "position:absolute;pointer-events:none;background:var(--card);border:1px solid color-mix(in oklch,var(--border) 50%,transparent);border-radius:4px;padding:0.35rem 0.5rem;font-size:0.7rem;color:var(--foreground);opacity:0;transition:opacity 120ms;z-index:10;";
  wrap.append(tooltip);

  // Gradient
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  const grad = document.createElementNS("http://www.w3.org/2000/svg", "linearGradient");
  grad.id = "wx-grad";
  grad.setAttribute("x1", "0");
  grad.setAttribute("y1", "0");
  grad.setAttribute("x2", "0");
  grad.setAttribute("y2", "1");
  const stop1 = document.createElementNS("http://www.w3.org/2000/svg", "stop");
  stop1.setAttribute("offset", "0%");
  stop1.setAttribute("stop-color", "var(--primary)");
  stop1.setAttribute("stop-opacity", "0.25");
  const stop2 = document.createElementNS("http://www.w3.org/2000/svg", "stop");
  stop2.setAttribute("offset", "100%");
  stop2.setAttribute("stop-color", "var(--primary)");
  stop2.setAttribute("stop-opacity", "0");
  grad.append(stop1, stop2);
  defs.append(grad);
  svg.append(defs);

  // Y-axis labels
  const ticks = 4;
  for (let t = 0; t <= ticks; t++) {
    const val = maxT - (range * t) / ticks;
    const ty = padT + (t / ticks) * plotH;
    const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
    label.setAttribute("x", String(padL - 6));
    label.setAttribute("y", String(ty + 4));
    label.setAttribute("text-anchor", "end");
    label.setAttribute("font-size", "10");
    label.setAttribute("fill", "var(--muted-foreground)");
    label.textContent = Math.round(val) + "\u00B0";
    svg.append(label);
  }

  // X-axis day labels
  for (let i = 0; i < daily.length; i++) {
    const dt = new Date(daily[i].dt * 1000);
    const label = dt.toLocaleDateString(undefined, { weekday: "short" });
    const lx = x(i);
    const tx = document.createElementNS("http://www.w3.org/2000/svg", "text");
    tx.setAttribute("x", String(lx));
    tx.setAttribute("y", String(h - padB + 16));
    tx.setAttribute("text-anchor", "middle");
    tx.setAttribute("font-size", "10");
    tx.setAttribute("fill", "var(--muted-foreground)");
    tx.textContent = label;
    svg.append(tx);
  }

  // Area path
  const maxPts = maxs.map((v, i) => v !== null ? `${x(i)} ${y(v)}` : null).filter(Boolean);
  const minPts = mins.map((v, i) => v !== null ? `${x(i)} ${y(v)}` : null).filter(Boolean).reverse();
  const areaD = "M " + maxPts.join(" L ") + " L " + minPts.join(" L ") + " Z";
  const area = document.createElementNS("http://www.w3.org/2000/svg", "path");
  area.setAttribute("class", "weather-chart__area");
  area.setAttribute("d", areaD);
  area.style.fill = "url(#wx-grad)";
  svg.append(area);

  // Max line
  const maxD = maxPts.join(" L ");
  if (maxD) {
    const maxPath = document.createElementNS("http://www.w3.org/2000/svg", "path");
    maxPath.setAttribute("class", "weather-chart__path weather-chart__path--max");
    maxPath.setAttribute("d", "M " + maxD);
    maxPath.style.cssText = "fill:none;stroke:var(--primary);stroke-width:2;stroke-linecap:round;stroke-linejoin:round;";
    svg.append(maxPath);
  }

  // Min line
  const minD = mins.map((v, i) => v !== null ? `${x(i)} ${y(v)}` : null).filter(Boolean).join(" L ");
  if (minD) {
    const minPath = document.createElementNS("http://www.w3.org/2000/svg", "path");
    minPath.setAttribute("class", "weather-chart__path weather-chart__path--min");
    minPath.setAttribute("d", "M " + minD);
    minPath.style.cssText = "fill:none;stroke:var(--muted-foreground);stroke-width:1.5;opacity:0.7;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:4 4;";
    svg.append(minPath);
  }

  // Dots for max + min with hover tooltip
  for (let i = 0; i < daily.length; i++) {
    const dayName = new Date(daily[i].dt * 1000).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
    const tmax = maxs[i];
    const tmin = mins[i];

    if (tmax !== null) {
      const dot = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      dot.setAttribute("class", "weather-chart__dot");
      dot.setAttribute("cx", String(x(i)));
      dot.setAttribute("cy", String(y(tmax)));
      dot.setAttribute("r", "4");
      dot.style.cssText = "fill:var(--primary);cursor:pointer;";
      dot.addEventListener("mouseenter", (e) => showTooltip(e, tooltip, dayName, tmax, tmin));
      dot.addEventListener("mousemove", (e) => moveTooltip(e, tooltip));
      dot.addEventListener("mouseleave", () => hideTooltip(tooltip));
      svg.append(dot);
    }
    if (tmin !== null) {
      const dot = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      dot.setAttribute("class", "weather-chart__dot");
      dot.setAttribute("cx", String(x(i)));
      dot.setAttribute("cy", String(y(tmin)));
      dot.setAttribute("r", "4");
      dot.style.cssText = "fill:var(--muted-foreground);opacity:0.7;cursor:pointer;";
      dot.addEventListener("mouseenter", (e) => showTooltip(e, tooltip, dayName, tmax, tmin));
      dot.addEventListener("mousemove", (e) => moveTooltip(e, tooltip));
      dot.addEventListener("mouseleave", () => hideTooltip(tooltip));
      svg.append(dot);
    }
  }

  wrap.append(svg);
  return wrap;
}

function showTooltip(e: MouseEvent, tooltip: HTMLElement, day: string, tmax: number | null, tmin: number | null): void {
  const unit = "\u00B0";
  let html = `<strong>${day}</strong><br/>`;
  if (tmax !== null) html += `Max: ${Math.round(tmax)}${unit}<br/>`;
  if (tmin !== null) html += `Min: ${Math.round(tmin)}${unit}`;
  tooltip.innerHTML = html;
  tooltip.style.opacity = "1";
  moveTooltip(e, tooltip);
}

function moveTooltip(e: MouseEvent, tooltip: HTMLElement): void {
  const rect = tooltip.parentElement!.getBoundingClientRect();
  tooltip.style.left = (e.clientX - rect.left + 10) + "px";
  tooltip.style.top = (e.clientY - rect.top - 10) + "px";
}

function hideTooltip(tooltip: HTMLElement): void {
  tooltip.style.opacity = "0";
}

function buildMetricsGrid(
  cur: any,
  air: any,
  daily: any[],
  isImperial: boolean,
): HTMLElement {
  const grid = document.createElement("div");
  grid.className = "weather-metrics";
  grid.style.cssText = "display:grid;grid-template-columns:repeat(3,1fr);gap:0.5rem;min-width:0;";

  const unit = isImperial ? "\u00B0F" : "\u00B0C";

  function fmt(v: unknown): string {
    if (v === undefined || v === null || v === "null") return "--";
    return String(v);
  }

  const metrics: Array<{ icon: string; label: string; value: string }> = [];

  if (cur) {
    if (cur.humidity !== undefined && cur.humidity !== null)
      metrics.push({ icon: "drop-half-bottom", label: "Humidity", value: fmt(cur.humidity) + "%" });
    if (cur.wind_speed !== undefined && cur.wind_speed !== null) {
      const dir = cur.wind_deg !== undefined ? degToCardinal(cur.wind_deg) : "";
      metrics.push({ icon: "wind", label: "Wind", value: Math.round(cur.wind_speed) + (isImperial ? " mph" : " m/s") + (dir ? " " + dir : "") });
    }
    if (cur.pressure !== undefined && cur.pressure !== null)
      metrics.push({ icon: "gauge", label: "Pressure", value: fmt(cur.pressure) + " hPa" });
    if (cur.uvi !== undefined && cur.uvi !== null)
      metrics.push({ icon: "sun", label: "UV Index", value: fmt(cur.uvi) });
    if (cur.visibility !== undefined && cur.visibility !== null)
      metrics.push({ icon: "eye", label: "Visibility", value: Math.round(cur.visibility / 1000) + " km" });
    if (cur.dew_point !== undefined && cur.dew_point !== null)
      metrics.push({ icon: "droplets", label: "Dew Point", value: Math.round(cur.dew_point) + unit });
  }

  if (air?.main?.aqi !== undefined && air.main.aqi !== null) {
    const aqiLabels = ["Good", "Fair", "Moderate", "Poor", "Very Poor"];
    const aqiLabel = aqiLabels[air.main.aqi - 1] || "Unknown";
    metrics.push({ icon: "flower-lotus", label: "Air Quality", value: "AQI " + fmt(air.main.aqi) + " (" + aqiLabel + ")" });
  }

  // Sunrise/sunset from daily[0]
  const firstDaily = daily[0];
  if (firstDaily?.sunrise && firstDaily?.sunset) {
    const rise = new Date(firstDaily.sunrise * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
    const set = new Date(firstDaily.sunset * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
    metrics.push({ icon: "sunrise", label: "Sunrise", value: rise });
    metrics.push({ icon: "sunset", label: "Sunset", value: set });
  }

  for (const m of metrics) {
    const card = document.createElement("div");
    card.className = "zen-card weather-metric";
    card.style.cssText = "display:flex;flex-direction:column;align-items:center;padding:0.6rem 0.4rem;gap:0.25rem;text-align:center;min-width:0;";

    const ic = document.createElement("span");
    ic.className = "zen-icon weather-metric__icon";
    ic.dataset.icon = m.icon;
    ic.dataset.size = "20";
    setIcon(ic, m.icon, { size: 20 });

    const val = document.createElement("div");
    val.className = "weather-metric__value";
    val.style.cssText = "font-weight:600;font-size:0.9rem;color:var(--foreground);";
    val.textContent = m.value;

    const lbl = document.createElement("div");
    lbl.className = "weather-metric__label";
    lbl.style.cssText = "font-size:0.65rem;color:var(--muted-foreground);";
    lbl.textContent = m.label;

    card.append(ic, val, lbl);
    grid.append(card);
  }

  return grid;
}

function degToCardinal(deg: number): string {
  const dirs = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
  return dirs[Math.round(deg / 22.5) % 16];
}