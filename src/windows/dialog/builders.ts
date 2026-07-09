import type { DialogOptions, DialogContext, DialogAction } from "./base";
import type { CalendarEvent } from "../../shared/types";
import { registerDialog } from "./registry";
import { buildEventForm, type BuiltEventForm } from "../calendar/event-form";
// The event_edit builder renders calendar-specific controls (`.cal-event-form`,
// `.cal-weekday-btn`, …) whose styles live in `calendar.css`. The dialog
// window only loads `globals.css` by default, so we import the calendar
// styles here — the only dialog builder that needs them.
import "../calendar/calendar.css";

function action(
  label: string,
  variant: DialogAction["variant"],
  onClick: (ctx: DialogContext) => void | boolean | Promise<void | boolean>,
  opts: Partial<DialogAction> = {},
): DialogAction {
  return { label, variant, onClick, ...opts };
}

function renameBuilder(data: unknown): DialogOptions {
  const [id, currentName] = data as [number, string];

  const inputId = "dialog-rename-input";

  const body = (): HTMLElement => {
    const field = document.createElement("div");
    field.className = "zen-field";
    field.style.margin = "0";

    const label = document.createElement("label");
    label.className = "zen-label";
    label.textContent = "Desktop name";
    label.htmlFor = inputId;
    field.append(label);

    const input = document.createElement("input");
    input.id = inputId;
    input.className = "zen-input";
    input.type = "text";
    input.value = currentName;
    input.autofocus = true;
    field.append(input);

    const valueGetter = () => (input.value.trim());
    (field as any).__getValue = valueGetter;
    // Enter key is handled by the dialog's document-level keydown listener
    // (base.ts:mountDialog) — picks the first action with submitOnNonButton
    // enabled (the Rename button) and fires it.

    return field;
  };

  const actions: DialogAction[] = [
    action("Cancel", "outline", (ctx) => ctx.close()),
    action("Rename", "primary", async (ctx: DialogContext) => {
      const field = ctx.content.querySelector(".zen-field") as any;
      const name: string = typeof field?.__getValue === "function" ? field.__getValue() : "";
      if (!name) return false;
      try { await ctx.invoke("rename_desktop", { id, name }); }
      catch (e) { console.error("[rename] IPC failed:", e); return false; }
      return true;
    }, { autofocus: false }),
  ];

  return {
    title: "Rename Desktop",
    data,
    body,
    actions,
    disableContextMenu: true,
    closeOnEscape: true,
  };
}

function deleteBuilder(data: unknown): DialogOptions {
  const id = data as number;

  const body = document.createElement("div");
  body.style.cssText = "display:flex;flex-direction:column;gap:0.5rem";

  const label = document.createElement("p");
  label.className = "zen-label";
  label.textContent = `Delete Desktop ${id + 1}?`;
  label.style.cssText = "font-weight:600;margin:0";
  body.append(label);

  const hint = document.createElement("p");
  hint.className = "zen-hint";
  hint.textContent = "Windows will be moved to another desktop.";
  hint.style.cssText = "margin:0";
  body.append(hint);

  const actions: DialogAction[] = [
    action("Cancel", "outline", (ctx) => ctx.close()),
    action("Delete", "destructive", async (ctx: DialogContext) => {
      try { await ctx.invoke("delete_desktop", { id }); }
      catch (e) { console.error("[delete] IPC failed:", e); return false; }
      return true;
    }, { autofocus: true }),
  ];

  return {
    title: "Delete Desktop",
    data,
    body,
    actions,
    disableContextMenu: true,
    closeOnEscape: true,
  };
}

function eventEditBuilder(data: unknown): DialogOptions {
  const payload = (data as { event?: CalendarEvent | null } | null) ?? null;
  const existing = payload?.event ?? null;

  let form: BuiltEventForm | null = null;

  const body = (): HTMLElement => {
    form = buildEventForm(existing);
    return form.root;
  };

  const actions: DialogAction[] = [
    action("Cancel", "outline", (ctx) => ctx.close()),
    action(existing ? "Save" : "Add", "primary", async (ctx: DialogContext) => {
      if (!form) return false;
      const values = form.read();
      if (!values) return false; // title empty — keep dialog open
      try {
        if (existing) {
          await ctx.invoke("update_event", { event: { ...existing, ...values } });
        } else {
          await ctx.invoke("add_event", { event: values });
        }
        return true;
      } catch (e) {
        console.error("[event_edit] IPC failed:", e);
        return false;
      }
    }, { autofocus: true }),
  ];

  return {
    title: existing ? "Edit Event" : "New Event",
    data,
    body,
    actions,
    disableContextMenu: true,
    closeOnEscape: true,
  };
}

export function registerBuiltins(): void {
  registerDialog("rename", renameBuilder);
  registerDialog("delete", deleteBuilder);
  registerDialog("event_edit", eventEditBuilder);
}

registerBuiltins();
