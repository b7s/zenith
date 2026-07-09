import { applyIcons, setIcon } from "../../shared/icon";
import type { CalendarEvent } from "../../shared/types";

export interface CalendarState {
  year: number;
  month: number;
}

export interface CalendarMountOptions {
  showNextMonth?: boolean;
  todayBtn?: HTMLElement;
  events?: CalendarEvent[];
  /** Called when a day cell is clicked. Receives the date (YYYY-MM-DD). */
  onDayClick?: (date: string) => void;
}

export type CalendarChangeHandler = (state: CalendarState) => void;

const _monthFormatter = new Intl.DateTimeFormat(navigator.language, { month: "long" });
const _weekdayFormatter = new Intl.DateTimeFormat(navigator.language, { weekday: "short" });
const _refJan = new Date(2024, 0, 1);

function pad(n: number): string { return n < 10 ? `0${n}` : `${n}`; }

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
    panels.append(buildPanel(state, opts.events, opts.onDayClick));
    panels.append(buildPanel(nextMonthState, opts.events, opts.onDayClick));
    fragment.append(panels);
  } else {
    fragment.append(buildPanel(state, opts.events, opts.onDayClick));
  }

  parent.append(fragment);
  applyIcons(parent);
}

function buildPanel(state: CalendarState, events?: CalendarEvent[], onDayClick?: (date: string) => void): HTMLElement {
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

  // Group events by day number for this month
  const dayEvents = new Map<number, CalendarEvent[]>();
  if (events) {
    const prefix = `${state.year}-${pad(state.month + 1)}-`;
    for (const ev of events) {
      if (ev.date.startsWith(prefix)) {
        const dayStr = ev.date.slice(prefix.length);
        const day = parseInt(dayStr, 10);
        if (!isNaN(day)) {
          const list = dayEvents.get(day) || [];
          list.push(ev);
          dayEvents.set(day, list);
        }
      }
    }
  }

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

    // Event dots
    const evList = dayEvents.get(d);
    if (evList && evList.length > 0) {
      const dots = document.createElement("div");
      dots.className = "cal-day-dots";
      const count = Math.min(evList.length, 3);
      for (let i = 0; i < count; i++) {
        const dot = document.createElement("span");
        dot.className = "cal-day-dot";
        if (i === 2 && evList.length > 3) dot.classList.add("is-more");
        dots.append(dot);
      }
      cell.append(dots);
    }

    cell.addEventListener("click", () => {
      panel.querySelectorAll(".cal-day.is-selected").forEach((n) => n.classList.remove("is-selected"));
      cell.classList.add("is-selected");
      if (onDayClick) {
        const mm = pad(state.month + 1);
        const dd = pad(d);
        onDayClick(`${state.year}-${mm}-${dd}`);
      }
    });
    grid.append(cell);
  }
  panel.append(grid);

  return panel;
}

const WEEKDAY_LABELS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function weekdayLabel(bit: number): string {
  return WEEKDAY_LABELS[bit] ?? "";
}

/** Format a CalendarEvent for list display: "HH:MM" or "All day", plus recurrence. */
export function eventTimeLabel(ev: CalendarEvent): string {
  if (!ev.time) return "All day";
  let label = ev.time;
  if (ev.recurrence !== "none") {
    label += ` · ${ev.recurrence}`;
  }
  return label;
}

/** Build a single read-only row for the event list panel.
 *  `onEdit` / `onDelete` wire the action buttons. */
export function buildEventRow(
  ev: CalendarEvent,
  onEdit: (ev: CalendarEvent) => void,
  onDelete: (ev: CalendarEvent) => void,
): HTMLElement {
  const row = document.createElement("div");
  row.className = "cal-event-row";
  if (ev.kind === "alarm") row.classList.add("is-alarm");
  if (!ev.enabled) row.classList.add("is-disabled");

  if (ev.kind === "alarm") {
    const ic = document.createElement("span");
    ic.className = "cal-event-kindic";
    setIcon(ic, "alarm-clock", { size: 11 });
    row.append(ic);
  }

  const main = document.createElement("button");
  main.type = "button";
  main.className = "cal-event-main";
  main.addEventListener("click", () => onEdit(ev));

  const title = document.createElement("span");
  title.className = "cal-event-title";
  title.textContent = ev.title || "(untitled)";
  main.append(title);

  if (ev.notes) {
    const nText = document.createElement("span");
    nText.className = "cal-event-notes-preview";
    nText.textContent = ev.notes;
    nText.title = ev.notes;
    main.append(nText);
  }

  const meta = document.createElement("span");
  meta.className = "cal-event-meta";
  if (ev.recurrence !== "none") {
    const recIc = document.createElement("span");
    recIc.className = "cal-event-recic zen-icon";
    setIcon(recIc, "repeat", { size: 11 });
    meta.append(recIc);
  }
  const dateLabel = ev.recurrence === "none"
    ? ev.date
    : ev.recurrence === "weekly"
      ? `Weekly · ${weekdayBitToLabel(ev.weekdays)}`
      : ev.recurrence;
  const dateText = document.createElement("span");
  dateText.className = "cal-event-meta-text";
  dateText.textContent = `${eventTimeLabel(ev)} · ${dateLabel}`;
  meta.append(dateText);
  main.append(meta);

  row.append(main);

  const actions = document.createElement("div");
  actions.className = "cal-event-actions";

  const editBtn = document.createElement("button");
  editBtn.type = "button";
  editBtn.className = "zen-icon-button cal-event-btn";
  editBtn.setAttribute("aria-label", "Edit");
  editBtn.title = "Edit";
  setIcon(editBtn, "pencil", { size: 13 });
  editBtn.addEventListener("click", (e) => { e.stopPropagation(); onEdit(ev); });
  actions.append(editBtn);

  const delBtn = document.createElement("button");
  delBtn.type = "button";
  delBtn.className = "zen-icon-button cal-event-btn is-danger";
  delBtn.setAttribute("aria-label", "Delete");
  delBtn.title = "Delete";
  setIcon(delBtn, "trash-2", { size: 13 });
  delBtn.addEventListener("click", (e) => { e.stopPropagation(); onDelete(ev); });
  actions.append(delBtn);

  row.append(actions);
  return row;
}

function weekdayBitToLabel(mask: number): string {
  const names: string[] = [];
  for (let b = 0; b < 7; b++) {
    if (mask & (1 << b)) names.push(weekdayLabel(b));
  }
  return names.join(", ") || "—";
}

