import "../../styles/globals.css";
import "./calendar.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { initLog, logInfo } from "../../shared/log";
import type { Config, CalendarEvent } from "../../shared/types";
import {
  loadEvents,
  deleteEvent,
  openEventEditDialog,
  EVENT,
  type EventName,
} from "../../shared/events";
import {
  mountCalendar,
  addMonths,
  todayLocal,
  monthName,
  makeIconButton,
  populateYearOptions,
  buildEventRow,
  type CalendarState,
} from "./calendar";
import { mountFilterPills, type FilterPillsMount } from "../../shared/filter-pills";
import { CMD } from "../../shared/ipc";

type ViewMode = "calendar" | "events";
type EventFilter = "all" | "event" | "alarm";

void (async () => {
  await initLog();
  logInfo("calendar popup ready");

  const injected = window as unknown as { __ZENITH_CALENDAR_VIEW?: string };
  // Instant seed from the init script (available synchronously, before any
  // IPC roundtrip). Confirmed/refuted by the `get_calendar_view` IPC call
  // below — the IPC is the primary source of truth because the init script
  // can race with page load on some Tauri builds.
  let mode: ViewMode = injected.__ZENITH_CALENDAR_VIEW === "events" ? "events" : "calendar";
  let eventFilter: EventFilter = "all";
  let pillsMount: FilterPillsMount<EventFilter> | null = null;
  /** Date filter (YYYY-MM-DD or null). When set, only events on that day
   *  show, regardless of `eventFilter`. Cleared via the "All days" pill in
   *  the events header. */
  let dayFilter: string | null = null;

  // Raw config value — never mutated. `showNextMonth` below is derived
  // from this + the single-month override.
  let configShowNextMonth = false;
  let showNextMonth = false;
  let events: CalendarEvent[] = [];
  try {
    const [cfg, ev, view, forceSingle] = await Promise.all([
      invoke<Config>("get_config"),
      loadEvents(),
      invoke<string>("get_calendar_view"),
      invoke<boolean>("get_calendar_single"),
    ]);
    events = ev;
    const wc = cfg.widgets?.config?.["datetime"] as Record<string, unknown> | undefined;
    configShowNextMonth = Boolean(wc?.show_next_month);
    // IPC is the primary source of truth — overrides the init-script seed.
    mode = view === "events" ? "events" : "calendar";
    // Safety net: even in calendar mode, never show 2 months when the
    // caller forced single (alarms widget).
    showNextMonth = configShowNextMonth && !forceSingle;
  } catch {
    // Non-fatal — fall back to the init-script seed.
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

  // Left group — calendar view: [‹][›][Today].
  const leftGroup = document.createElement("div");
  leftGroup.className = "cal-hleft";

  const prevBtn = makeIconButton("caret-left", "Previous month");
  prevBtn.addEventListener("click", () => stepMonth(-1));
  leftGroup.append(prevBtn);

  const nextBtn = makeIconButton("caret-right", "Next month");
  nextBtn.addEventListener("click", () => stepMonth(1));
  leftGroup.append(nextBtn);

  // "Today" button lives in the header, right after the "Next month"
  // chevron. Visibility is driven by `mountCalendar`'s `opts.todayBtn`:
  // it hides the button when the current view already equals today.
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

  // Center: title. Calendar view shows month/year/(next month).
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

  // ---- events header group (events mode: title + filter pills left-aligned) --
  const eventsHeaderGroup = document.createElement("div");
  eventsHeaderGroup.className = "cal-events-header";

  // Day filter chip — only in events view, only when a day is filtered.
  const dayFilterChip = document.createElement("button");
  dayFilterChip.type = "button";
  dayFilterChip.className = "zen-icon-button cal-day-filter-chip";
  dayFilterChip.setAttribute("aria-label", "Clear day filter");
  dayFilterChip.title = "Clear day filter";
  const dayFilterIcon = document.createElement("span");
  dayFilterIcon.className = "zen-icon";
  dayFilterChip.append(dayFilterIcon);
  const dayFilterLabel = document.createElement("span");
  dayFilterLabel.className = "cal-day-filter-chip-label";
  dayFilterChip.append(dayFilterLabel);
  setIcon(dayFilterChip, "x", { size: 12 });
  dayFilterChip.addEventListener("click", () => {
    if (dayFilter === null) return;
    dayFilter = null;
    render();
  });
  eventsHeaderGroup.append(dayFilterChip);

  // Filter pills mount point — populated once in renderEventsView()
  const pillsWrap = document.createElement("div");
  pillsWrap.className = "cal-event-filter";
  eventsHeaderGroup.append(pillsWrap);

  header.append(eventsHeaderGroup);

  // Right: settings + toggle view + add + close
  const rightGroup = document.createElement("div");
  rightGroup.className = "cal-hright";

  // Settings (cog) — opens the datetime widget config (calendar accounts,
  // appearance, etc.) in the same style as the other header icon buttons.
  const settingsBtn = makeIconButton("config", "Calendar settings");
  settingsBtn.addEventListener("click", () => {
    void invoke(CMD.openWidgetConfig, { widgetId: "datetime" });
  });
  rightGroup.append(settingsBtn);

  // View toggle button (calendar <-> events). Hidden in events-only mode.
  const viewToggle = makeIconButton("calendar-search", "Show events");
  viewToggle.addEventListener("click", () => {
    mode = mode === "calendar" ? "events" : "calendar";
    render();
  });
  rightGroup.append(viewToggle);

  const addBtn = makeIconButton("plus", "Add event");
  addBtn.addEventListener("click", () => {
    void openEventEditDialog(null);
  });
  rightGroup.append(addBtn);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "zen-icon-button zen-window__close";
  close.setAttribute("aria-label", "Close");
  close.title = "Close";
  setIcon(close, "x", { size: 14 });
  close.addEventListener("click", () => {
    void getCurrentWindow().close().catch(() => window.close());
  });
  rightGroup.append(close);

  header.append(rightGroup);

  const content = document.createElement("main");
  content.className = "zen-window__content cal-window__content";

  wrapper.append(header, content);
  root.replaceChildren(wrapper);

  // ----- State -----
  let state: CalendarState = todayLocal();
  let slideDir: 0 | 1 | -1 = 0;

  function stepMonth(delta: number): void {
    state = addMonths(state, delta);
    slideDir = delta > 0 ? 1 : -1;
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

  async function refreshEvents(): Promise<void> {
    try {
      events = await loadEvents();
    } catch {
      events = [];
    }
  }

  function applyModeChrome(): void {
    const inEvents = mode === "events";
    // Left nav: calendar view only
    leftGroup.style.display = inEvents ? "none" : "";
    // Center title group: calendar mode only
    centerGroup.style.display = inEvents ? "none" : "";
    // Events header group: events mode only
    eventsHeaderGroup.style.display = inEvents ? "flex" : "none";
    // Day filter chip — only in events view, only when a day is filtered.
    if (inEvents && dayFilter) {
      dayFilterLabel.textContent = `Day · ${dayFilter}`;
      dayFilterChip.style.display = "inline-flex";
    } else {
      dayFilterChip.style.display = "none";
    }
    // Toggle: in calendar mode it's "Show events" (calendar->events),
    // in events mode it's "Show calendar" (events->calendar).
    if (inEvents) {
      viewToggle.title = "Show calendar";
      viewToggle.setAttribute("aria-label", "Show calendar");
      setIcon(viewToggle, "calendar", { size: 14 });
    } else {
      viewToggle.title = "Show events";
      viewToggle.setAttribute("aria-label", "Show events");
      setIcon(viewToggle, "calendar-search", { size: 14 });
    }
  }

  function render(): void {
    applyModeChrome();

    if (mode === "events") {
      renderEventsView();
    } else {
      renderCalendarView();
    }
  }

  function renderCalendarView(): void {
    monthLabel.textContent = monthName(state.month);
    yearSelect.value = String(state.year);
    if (showNextMonth) {
      const next = addMonths(state, 1);
      nextMonthLabel.textContent = monthName(next.month);
    } else {
      nextMonthLabel.textContent = "";
    }

    const grid = document.createElement("div");
    grid.className = "cal-grid-wrap";
    if (slideDir === 1) grid.classList.add("is-slide-next");
    else if (slideDir === -1) grid.classList.add("is-slide-prev");
    mountCalendar(grid, state, (_next) => {
      state = _next;
      render();
    }, {
      showNextMonth,
      todayBtn,
      events,
      onDayClick: (date) => {
        dayFilter = date;
        mode = "events";
        render();
      },
    });

    const body = document.createElement("div");
    body.className = "cal-view cal-view--calendar";
    body.append(grid);

    content.replaceChildren(body);
    slideDir = 0;
  }

  function renderEventsView(): void {
    // Mount (once) and reuse the shared segmented control so the active
    // pill class survives re-renders.
    if (!pillsMount) {
      pillsMount = mountFilterPills<EventFilter>(
        pillsWrap,
        [
          { id: "all",   label: "All" },
          { id: "event", label: "Event" },
          { id: "alarm", label: "Alarm" },
        ],
        eventFilter,
      );
      // Controlled state: parent owns `eventFilter`, click only flips it,
      // then `render()` rebuilds the list below.
      pillsMount.container.addEventListener("click", (e) => {
        const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-pill-id]");
        if (!btn) return;
        const next = btn.dataset.pillId as EventFilter;
        if (next !== eventFilter) {
          eventFilter = next;
          render();
        }
      });
    } else {
      pillsMount.switchTo(eventFilter);
    }

    const list = document.createElement("div");
    list.className = "cal-event-list";

    const nowMs = Date.now();
    const HOUR_MS = 60 * 60 * 1000;

    const sorted = events
      .filter((e) => eventFilter === "all" ? true : e.kind === eventFilter)
      .filter((e) => dayFilter ? e.date === dayFilter : true)
      // Drop fully-expired entries. All-day events show only today and
      // future days; timed one-shots older than 1 hour fade out so the
      // list stays focused on the upcoming actionable entries.
      .filter((e) => {
        if (e.recurrence !== "none") return true;
        if (!e.time) {
          const endOfDay = new Date(e.date + "T23:59:59").getTime();
          const midnightToday = new Date();
          midnightToday.setHours(0, 0, 0, 0);
          return endOfDay >= midnightToday.getTime();
        }
        const scheduled = new Date(e.date + `T${e.time}`).getTime();
        return scheduled >= nowMs - HOUR_MS;
      })
      .sort((a, b) => {
        const da = new Date(a.date + (a.time ? `T${a.time}` : "T00:00")).getTime();
        const db = new Date(b.date + (b.time ? `T${b.time}` : "T00:00")).getTime();
        return da - db;
      });

    if (sorted.length === 0) {
      const empty = document.createElement("div");
      empty.className = "cal-event-empty";
      empty.style.setProperty("--cal-row-i", "0");
      const iconWrap = document.createElement("span");
      iconWrap.className = "cal-event-empty-icon";
      setIcon(iconWrap, "calendar", { size: 22 });
      const msg = document.createElement("span");
      msg.className = "cal-event-empty-msg";
      msg.textContent = dayFilter
        ? `Nothing scheduled on ${dayFilter}.`
        : eventFilter === "all"
          ? "No events yet — click + to add one."
          : eventFilter === "alarm"
            ? "No alarms — click + to add one."
            : "No events — click + to add one.";
      empty.append(iconWrap, msg);
      list.append(empty);
    } else {
      let i = 0;
      for (const ev of sorted) {
        const row = buildEventRow(ev, openEventEdit, doDelete);
        row.style.setProperty("--cal-row-i", String(Math.min(i, 8)));
        list.append(row);
        i++;
      }
    }

    content.replaceChildren(list);
  }

  async function openEventEdit(ev: CalendarEvent): Promise<void> {
    await openEventEditDialog(ev);
  }

  async function doDelete(ev: CalendarEvent): Promise<void> {
    try {
      await deleteEvent(ev.id);
      await refreshEvents();
      render();
    } catch {
      // Non-fatal.
    }
  }

  // ----- Close on Escape -----
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      void getCurrentWindow().close().catch(() => {});
    }
  });

  // ----- Close on blur (only when no overlay is open) -----
  const win = getCurrentWindow();
  void win.onFocusChanged(({ payload }) => {
    if (payload === false) {
      void win.close().catch(() => {});
    }
  });

  const m = await import("@tauri-apps/api/event");
  const unlisten = m.listen(EVENT.eventsUpdated as EventName, () => {
    void refreshEvents().then(() => render());
  });

  // Live-update the panel count when the user toggles
  // `show_next_month` from the widget-config window while the calendar
  // is already open. Refreshes the cached config value + re-renders,
  // so the wide↔single swap happens without a reopen.
  const unlistenConfig = m.listen(EVENT.configUpdated as EventName, async () => {
    try {
      const [cfg, forceSingle] = await Promise.all([
        invoke<Config>("get_config"),
        invoke<boolean>("get_calendar_single"),
      ]);
      const wc = cfg.widgets?.config?.["datetime"] as Record<string, unknown> | undefined;
      configShowNextMonth = Boolean(wc?.show_next_month);
      showNextMonth = configShowNextMonth && !forceSingle;
      render();
    } catch {
      // keep current value
    }
  });

  // Switch view mode when the window is reused by a different caller
  // (e.g. datetime widget opened the 2-month grid, then the alarms widget
  // asks for the events list). The init script seeds the initial mode on
  // a fresh open; this listener handles subsequent reuses.
  const unlistenView = m.listen<string>(EVENT.calendarView as EventName, async (e) => {
    const next = e.payload === "events" ? "events" : "calendar";
    // Re-fetch BOTH the live config (the user may have toggled
    // `show_next_month` since the last open) and the single flag (the
    // alarms widget forces single → no 2 months even in calendar mode).
    // The cached `configShowNextMonth` from the window's first load is
    // never updated otherwise, so a wide→single config change would be
    // silenced on the reuse path.
    try {
      const [cfg, forceSingle] = await Promise.all([
        invoke<Config>("get_config"),
        invoke<boolean>("get_calendar_single"),
      ]);
      const wc = cfg.widgets?.config?.["datetime"] as Record<string, unknown> | undefined;
      configShowNextMonth = Boolean(wc?.show_next_month);
      showNextMonth = configShowNextMonth && !forceSingle;
    } catch {
      // keep current value
    }
    mode = next;
    render();
  });

  await refreshEvents();
  render();

  void unlisten;
  void unlistenView;
  void unlistenConfig;
})();
