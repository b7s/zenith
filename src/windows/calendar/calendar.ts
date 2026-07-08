import { applyIcons, setIcon } from "../../shared/icon";

export interface CalendarState {
  year: number;
  month: number;
}

export interface CalendarMountOptions {
  showNextMonth?: boolean;
  todayBtn?: HTMLElement;
}

export type CalendarChangeHandler = (state: CalendarState) => void;

const _monthFormatter = new Intl.DateTimeFormat(navigator.language, { month: "long" });
const _weekdayFormatter = new Intl.DateTimeFormat(navigator.language, { weekday: "short" });
const _refJan = new Date(2024, 0, 1);

export function monthName(m: number): string {
  return _monthFormatter.format(new Date(2024, m, 1));
}

function weekdayShort(d: number): string {
  const ref = new Date(_refJan);
  ref.setDate(ref.getDate() + (d + 6) % 7);
  return _weekdayFormatter.format(ref);
}

export function todayLocal(): CalendarState {
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth() };
}

function daysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}

function firstWeekday(year: number, month: number): number {
  return new Date(year, month, 1).getDay();
}

export function isSameYM(a: CalendarState, b: CalendarState): boolean {
  return a.year === b.year && a.month === b.month;
}

function sameDate(state: CalendarState, day: number): boolean {
  const t = todayLocal();
  return isSameYM(state, t) && day === new Date().getDate();
}

export function addMonths(state: CalendarState, delta: number): CalendarState {
  let m = state.month + delta;
  let y = state.year;
  if (m < 0) { m = 11; y -= 1; }
  else if (m > 11) { m = 0; y += 1; }
  return { year: y, month: m };
}

export function makeIconButton(name: string, label: string): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "zen-icon-button cal-nav";
  btn.setAttribute("aria-label", label);
  btn.title = label;
  setIcon(btn, name, { size: 14 });
  return btn;
}

export function populateYearOptions(select: HTMLSelectElement, around: number): void {
  const FUTURE = 10;
  const PAST = 29; // 29 past + current + 10 future = 40 options
  const start = around - PAST;
  const end = around + FUTURE;
  select.replaceChildren();
  for (let y = end; y >= start; y--) {
    const o = document.createElement("option");
    o.value = String(y);
    o.textContent = String(y);
    select.append(o);
  }
}

/** Render just the calendar panels (weekday grid + day grid).
 *  Clears parent and rebuilds. Also updates Today button visibility. */
export function mountCalendar(
  parent: HTMLElement,
  state: CalendarState,
  _cb: CalendarChangeHandler,
  opts: CalendarMountOptions = {},
): void {
  parent.replaceChildren();

  const showNext = Boolean(opts.showNextMonth);
  const today = todayLocal();
  const nextMonthState = addMonths(state, 1);

  const todayVisible = !(isSameYM(state, today) ||
    (showNext && isSameYM(nextMonthState, today)));
  if (opts.todayBtn) {
    opts.todayBtn.style.display = todayVisible ? "" : "none";
  }

  const fragment = document.createDocumentFragment();

  if (showNext) {
    const panels = document.createElement("div");
    panels.className = "cal-panels";
    panels.append(buildPanel(state));
    panels.append(buildPanel(nextMonthState));
    fragment.append(panels);
  } else {
    fragment.append(buildPanel(state));
  }

  parent.append(fragment);
  applyIcons(parent);
}

function buildPanel(state: CalendarState): HTMLElement {
  const panel = document.createElement("section");
  panel.className = "cal-panel";

  const wkHead = document.createElement("div");
  wkHead.className = "cal-weekdays";
  for (let d = 0; d < 7; d++) {
    const span = document.createElement("span");
    span.textContent = weekdayShort(d);
    wkHead.append(span);
  }
  panel.append(wkHead);

  const grid = document.createElement("div");
  grid.className = "cal-grid";
  const dim = daysInMonth(state.year, state.month);
  const first = firstWeekday(state.year, state.month);

  for (let i = 0; i < first; i++) {
    const blank = document.createElement("span");
    blank.className = "cal-day is-blank";
    grid.append(blank);
  }

  for (let d = 1; d <= dim; d++) {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "cal-day";
    if (sameDate(state, d)) cell.classList.add("is-today");
    cell.textContent = String(d);
    cell.addEventListener("click", () => {
      panel.querySelectorAll(".cal-day.is-selected").forEach((n) => n.classList.remove("is-selected"));
      cell.classList.add("is-selected");
    });
    grid.append(cell);
  }
  panel.append(grid);

  return panel;
}
