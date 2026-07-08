import "../../styles/globals.css";
import "./calendar.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { initLog, logInfo } from "../../shared/log";
import type { Config } from "../../shared/types";
import {
  mountCalendar,
  addMonths,
  todayLocal,
  monthName,
  makeIconButton,
  populateYearOptions,
  type CalendarState,
} from "./calendar";

void (async () => {
  await initLog();
  logInfo("calendar popup ready");

  let showNextMonth = false;
  try {
    const cfg = await invoke<Config>("get_config");
    const wc = cfg.widgets?.config?.["datetime"] as Record<string, unknown> | undefined;
    showNextMonth = Boolean(wc?.show_next_month);
  } catch {
    // Non-fatal.
  }

  const root = document.getElementById("root");
  if (!root) return;

  const { applyTheme } = await import("../../shared/window");
  await applyTheme();
  const { setIcon } = await import("../../shared/icon");

  // ----- Chrome -----
  const wrapper = document.createElement("div");
  wrapper.className = "zen-window cal-window";

  const header = document.createElement("header");
  header.className = "zen-window__header cal-window__header";

  // Left group: [‹][›][Today]
  const leftGroup = document.createElement("div");
  leftGroup.className = "cal-hleft";

  const prevBtn = makeIconButton("chevron-left", "Previous month");
  prevBtn.addEventListener("click", () => stepMonth(-1));
  leftGroup.append(prevBtn);

  const nextBtn = makeIconButton("chevron-right", "Next month");
  nextBtn.addEventListener("click", () => stepMonth(1));
  leftGroup.append(nextBtn);

  const todayBtn = document.createElement("button");
  todayBtn.type = "button";
  todayBtn.className = "cal-today";
  todayBtn.textContent = "Today";
  todayBtn.addEventListener("click", () => {
    state = todayLocal();
    render();
  });
  leftGroup.append(todayBtn);

  header.append(leftGroup);

  // Center: month name(s) + year select
  const centerGroup = document.createElement("div");
  centerGroup.className = "cal-hcenter";

  const monthLabel = document.createElement("span");
  monthLabel.className = "cal-month";
  centerGroup.append(monthLabel);

  const yearWrapper = document.createElement("span");
  yearWrapper.className = "cal-year-wrapper";
  const yearSelect = document.createElement("select");
  yearSelect.className = "cal-year";
  yearSelect.setAttribute("aria-label", "Year");
  populateYearOptions(yearSelect, new Date().getFullYear());
  yearSelect.addEventListener("change", () => {
    const y = parseInt(yearSelect.value, 10);
    if (Number.isFinite(y)) {
      state = { year: y, month: state.month };
      render();
    }
  });
  yearWrapper.append(yearSelect);
  centerGroup.append(yearWrapper);

  const nextMonthLabel = document.createElement("span");
  nextMonthLabel.className = "cal-month";
  centerGroup.append(nextMonthLabel);

  header.append(centerGroup);

  // Right: close
  const close = document.createElement("button");
  close.type = "button";
  close.className = "zen-icon-button zen-window__close";
  close.setAttribute("aria-label", "Close");
  close.title = "Close";
  setIcon(close, "x", { size: 14 });
  close.addEventListener("click", () => {
    void getCurrentWindow().close().catch(() => window.close());
  });
  header.append(close);

  const content = document.createElement("main");
  content.className = "zen-window__content cal-window__content";

  wrapper.append(header, content);
  root.replaceChildren(wrapper);

  // ----- State -----
  let state: CalendarState = todayLocal();

  function stepMonth(delta: number): void {
    state = addMonths(state, delta);
    const newestOption = yearSelect.options[0]?.value;
    const oldestOption = yearSelect.options[yearSelect.options.length - 1]?.value;
    if (
      state.year > parseInt(newestOption ?? String(state.year), 10) ||
      state.year < parseInt(oldestOption ?? String(state.year), 10)
    ) {
      populateYearOptions(yearSelect, state.year);
    }
    render();
  }

  function render(): void {
    // Update title
    monthLabel.textContent = monthName(state.month);
    yearSelect.value = String(state.year);

    if (showNextMonth) {
      const next = addMonths(state, 1);
      nextMonthLabel.textContent = monthName(next.month);
      nextMonthLabel.style.display = "";
    } else {
      nextMonthLabel.style.display = "none";
    }

    mountCalendar(content, state, (_next) => {
      state = _next;
      render();
    }, { showNextMonth, todayBtn });
  }
  render();

  // ----- Close on Escape -----
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      void getCurrentWindow().close().catch(() => {});
    }
  });

  // ----- Close on blur -----
  const win = getCurrentWindow();
  void win.onFocusChanged(({ payload }) => {
    if (payload === false) {
      void win.close().catch(() => {});
    }
  });
})();
