import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { CMD } from "./ipc";
import { loadConfig } from "./config";
import type { Config } from "./types";

/** Popup logical (CSS) dimensions — kept here so the positioning math and
 *  the IPC command proposal stay in sync. Mirrors of the Rust `CALENDAR_W`
 *  / `CALENDAR_H` / `CALENDAR_W_WIDE` in
 *  `src-tauri/src/calendar/commands.rs`.
 *  On Windows with default DPI these equal the OS-pixel size; on high-DPI
 *  systems the Rust side multiplies by its own DPI awareness. */
export const CALENDAR_POPUP_CSS_W = 340;
export const CALENDAR_POPUP_CSS_H = 370;
export const CALENDAR_POPUP_CSS_W_WIDE = 680;

/**
 * Resolve a widget element to OS-pixel screen coordinates where the popup
 * should be PLACED for it to appear **centered** under the widget.
 *
 * Returns the *top-left* of where the popup rectangle must be drawn so
 * that:
 *   - horizontally: popup.center.x === widget.center.x
 *   - vertically:   popup.top.y   === widget.bottom.y + `gapBelowPx`
 *
 * Once this proposal is computed, `crate::window::clamp_to_monitor` in
 * Rust snap-fits the rectangle inside the target monitor. If the popup
 * would overflow on either axis, the clamp shifts it inside; if it would
 * overflow onto the next monitor, it stays on the requester's monitor and
 * the user's mouse position remains visually correct. The frontend cannot
 * reliably predict how much overflow there will be without re-enumerating
 * monitors, so we let the clamp do the final say.
 *
 * The bar window's `outerPosition()` is in OS physical pixels. The widget's
 * `getBoundingClientRect()` is in CSS pixels of the bar's WebView.
 * `devicePixelRatio` converts between the two.
 */
export async function popupAnchorUnderWidget(
  widget: HTMLElement,
  popupW: number,
  gapBelowPx = 4,
): Promise<{ x: number; y: number }> {
  const bar = getCurrentWindow();
  const winPos = await bar.outerPosition();
  const dpr = window.devicePixelRatio || 1;
  const r = widget.getBoundingClientRect();

  // CSS-pixel center of the widget...
  const widgetCenterCss = r.left + r.width / 2;
  const widgetBottomCss = r.bottom + gapBelowPx;

  // ...converted to OS physical pixels and offset by the bar window's
  // own origin on the virtual desktop.
  const cx = winPos.x + widgetCenterCss * dpr;
  // Proposed top-left so popup ends up horizontally centered.
  const x = Math.round(cx - (popupW * dpr) / 2);
  const y = Math.round(winPos.y + widgetBottomCss * dpr);

  return { x, y };
}

async function shouldShowNextMonth(): Promise<boolean> {
  try {
    const cfg = (await loadConfig()) as Config;
    const wcfg = cfg.widgets?.config?.["datetime"] as
      | Record<string, unknown>
      | undefined;
    return Boolean(wcfg?.show_next_month);
  } catch {
    return false;
  }
}

/** Open the calendar popup, centered under the date/time widget. */
export async function openCalendarFromWidget(widget: HTMLElement): Promise<void> {
  const wide = await shouldShowNextMonth();
  const popupW = wide ? CALENDAR_POPUP_CSS_W_WIDE : CALENDAR_POPUP_CSS_W;
  const { x, y } = await popupAnchorUnderWidget(widget, popupW, 4);
  await invoke(CMD.openCalendar, { x, y, wide });
}

/** Open the calendar popup in events-only mode (no month grid, used by the
 *  alarms widget). Centered under the trigger widget. */
export async function openEventsPopupFromWidget(widget: HTMLElement, popupW = 320): Promise<void> {
  const { x, y } = await popupAnchorUnderWidget(widget, popupW, 4);
  await invoke(CMD.openCalendar, { x, y, wide: false, mode: "events" });
}

/** Popup dimensions mirroring the Rust `create_weather_window`. */
export const WEATHER_POPUP_CSS_W = 380;
export const WEATHER_POPUP_CSS_H = 560;

/** Open the weather forecast popup, centered under the widget. */
export async function openWeatherFromWidget(widget: HTMLElement): Promise<void> {
  const { x, y } = await popupAnchorUnderWidget(widget, WEATHER_POPUP_CSS_W, 4);
  await invoke(CMD.openWeather, { x, y });
}
