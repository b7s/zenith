import { invoke } from "@tauri-apps/api/core";
import type { CalendarEvent } from "./types";
import { CMD } from "./ipc";

// ---- Event name constants (single source of truth for cross-window events) ----
export const EVENT = {
  configUpdated: "zenith:config-updated",
  appearanceChanged: "zenith:appearance-changed",
  arrangeMode: "zenith:arrange-mode",
  widgetsChanged: "zenith:widgets-changed",
  workspaceChanged: "zenith:workspace-changed",
  workspaceRename: "zenith:workspace-rename",
  workspaceDelete: "zenith:workspace-delete",
  workspaceCreate: "zenith:workspace-create",
  workspaceMoveHere: "zenith:workspace-move-here",
  workspaceMoveTo: "zenith:workspace-move-to",
  workspaceTogglePin: "zenith:workspace-toggle-pin",
  crossDragStart: "zenith:cross-drag-start",
  crossDragMove: "zenith:cross-drag-move",
  crossDragEnd: "zenith:cross-drag-end",
  eventsUpdated: "zenith:events-updated",
  calendarView: "zenith:calendar-view",
} as const;

export type EventName = (typeof EVENT)[keyof typeof EVENT];

export interface CrossDragPayload {
  id: string;
  x?: number;
  y?: number;
}

// ---- Calendar event helpers (zenith local event store) ----
export async function loadEvents(): Promise<CalendarEvent[]> {
  return invoke<CalendarEvent[]>(CMD.getEvents);
}

export async function addEvent(
  event: Omit<CalendarEvent, "id" | "created_at" | "updated_at"> & Partial<Pick<CalendarEvent, "id">>,
): Promise<CalendarEvent> {
  return invoke<CalendarEvent>(CMD.addEvent, { event });
}

export async function updateEvent(event: CalendarEvent): Promise<CalendarEvent> {
  return invoke<CalendarEvent>(CMD.updateEvent, { event });
}

export async function deleteEvent(id: string): Promise<boolean> {
  return invoke<boolean>(CMD.deleteEvent, { id });
}

export async function syncEvents(): Promise<void> {
  await invoke(CMD.syncEvents);
}

/** Compute the next epoch-ms occurrence of an event (snap to local time).
 *  Used by widgets to display the next upcoming item without an extra
 *  backend round-trip. */
export function nextOccurrence(ev: CalendarEvent): Date | null {
  const time = ev.time ?? "00:00";
  const [hh, mm] = time.split(":").map((p) => parseInt(p, 10));
  if (Number.isNaN(hh) || Number.isNaN(mm)) return null;
  const now = new Date();
  const [yy, mo, dd] = ev.date.split("-").map((p) => parseInt(p, 10));
  if (Number.isNaN(yy) || Number.isNaN(mo) || Number.isNaN(dd)) return null;
  const base = new Date(yy, mo - 1, dd, hh, mm, 0, 0);
  switch (ev.recurrence) {
    case "none":
      return base >= now ? base : null;
    case "daily": {
      const c = new Date(now);
      c.setHours(hh, mm, 0, 0);
      if (c < now) c.setDate(c.getDate() + 1);
      return c;
    }
    case "weekly": {
      if (!ev.weekdays) return null;
      const c = new Date(now);
      c.setHours(hh, mm, 0, 0);
      for (let i = 0; i < 14; i++) {
        const bit = 1 << c.getDay();
        if ((ev.weekdays & bit) !== 0 && c >= now) return c;
        c.setDate(c.getDate() + 1);
      }
      return null;
    }
    case "monthly": {
      const c = new Date(now);
      c.setDate(dd);
      c.setHours(hh, mm, 0, 0);
      if (c < now) c.setMonth(c.getMonth() + 1);
      return c;
    }
  }
}

/** Open the unified event-edit dialog window. Pass `null` to add a new
 *  event; pass the existing event to edit it. Resolves once the user
 *  closes the dialog (either via Save / Cancel / Esc / ×). The backend
 *  already emits `zenith:events-updated` on save, so this function does
 *  not need to refresh state itself.
 *
 *  Position: centered on the **primary monitor's work area** (Rust
 *  `create_dialog_window` does this when no anchor is supplied — the
 *  monitor-aware anchor logic was unreliable across different DPI
 *  scales and multi-monitor topologies, so we default to the safe,
 *  predictable "center of the screen" behaviour).
 *
 *  Implementation note: the calendar/events popup is a custom HTML
 *  window — it doesn't go through the dialog flow. Only the calendar
 *  popup uses this helper to invoke an add/edit dialog without leaving
 *  the popup window. The dialog is opened via the backend `show_dialog`
 *  command (which creates a `dialog-event_edit` window). */
export async function openEventEditDialog(event: CalendarEvent | null): Promise<void> {
  await invoke("show_dialog", {
    spec: {
      kind: "event_edit",
      data: { event },
      width: 400,
      height: 600,
    },
  });
}
