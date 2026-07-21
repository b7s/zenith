import type { CalendarEvent, CalendarSource } from "../../shared/types";

export interface BuiltEventForm {
  root: HTMLElement;
  /** Read the current form values. Returns null if `title` is empty. */
  read(): Omit<CalendarEvent, "id" | "created_at" | "updated_at"> | null;
}

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const RECURRENCE_OPTIONS: { value: CalendarEvent["recurrence"]; label: string }[] = [
  { value: "none", label: "Once" },
  { value: "daily", label: "Daily" },
  { value: "weekly", label: "Weekly" },
  { value: "monthly", label: "Monthly" },
];

/** Build the event editor form. Returns the wrapper + a read() getter that
 *  pulls fresh values on demand (so callers can grab the snapshot at click
 *  time). All controls use `.zen-*` primitives — see AGENTS.md §6. */
export function buildEventForm(existing: CalendarEvent | null): BuiltEventForm {
  const root = document.createElement("div");
  root.className = "cal-event-form";

  // Enabled: canonical `.zen-checkbox` switch (label wraps a hidden
  // checkbox input + the `__switch` visual with `__track`/`__thumb`).
  const enabledField = document.createElement("label");
  enabledField.className = "zen-checkbox";
  const enabledText = document.createElement("div");
  enabledText.className = "zen-checkbox__text";
  const enabledTitle = document.createElement("span");
  enabledTitle.className = "zen-checkbox__label";
  enabledTitle.textContent = "Enabled";
  enabledText.append(enabledTitle);
  const enabledSwitch = document.createElement("span");
  enabledSwitch.className = "zen-checkbox__switch";
  let enabledVal = existing?.enabled ?? true;
  if (enabledVal) enabledSwitch.classList.add("is-on");
  const enabledInput = document.createElement("input");
  enabledInput.type = "checkbox";
  enabledInput.checked = enabledVal;
  const track = document.createElement("span");
  track.className = "zen-checkbox__track";
  const thumb = document.createElement("span");
  thumb.className = "zen-checkbox__thumb";
  track.append(thumb);
  enabledSwitch.append(enabledInput, track);
  enabledInput.addEventListener("change", () => {
    enabledVal = enabledInput.checked;
    enabledSwitch.classList.toggle("is-on", enabledVal);
  });
  enabledField.append(enabledText, enabledSwitch);
  root.append(enabledField);

  const titleInput = document.createElement("input");
  titleInput.type = "text";
  titleInput.className = "zen-input";
  titleInput.placeholder = "Title";
  titleInput.value = existing?.title ?? "";
  root.append(field("Title", titleInput));

  const dateInput = document.createElement("input");
  dateInput.type = "date";
  dateInput.className = "zen-input";
  dateInput.value = existing?.date ?? new Date().toISOString().slice(0, 10);
  root.append(field("Date", dateInput));

  const timeInput = document.createElement("input");
  timeInput.type = "time";
  timeInput.className = "zen-input";
  timeInput.value = existing?.time ?? "";
  root.append(field("Time (empty = all day)", timeInput));

  // Weekdays picker — created here (before Repeat) so the Repeat
  // onChange closure can reference it, but appended to root AFTER
  // Repeat so it visually follows it. Hidden unless Repeat == Weekly.
  const weekdayWrap = document.createElement("div");
  weekdayWrap.className = "zen-field cal-weekday-pick";
  weekdayWrap.style.display = (existing?.recurrence ?? "none") === "weekly" ? "" : "none";
  const wdLabel = document.createElement("label");
  wdLabel.className = "zen-label";
  wdLabel.textContent = "Weekdays";
  weekdayWrap.append(wdLabel);
  let weekdayMask = existing?.weekdays ?? 0;
  const wdRow = document.createElement("div");
  wdRow.className = "cal-weekday-row";
  WEEKDAYS.forEach((name, bit) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "cal-weekday-btn";
    b.textContent = name;
    if (weekdayMask & (1 << bit)) b.classList.add("is-on");
    b.addEventListener("click", () => {
      weekdayMask ^= 1 << bit;
      b.classList.toggle("is-on", Boolean(weekdayMask & (1 << bit)));
    });
    wdRow.append(b);
  });
  weekdayWrap.append(wdRow);

  const typeGroup = radioGroup<CalendarEvent["kind"]>(
    "Type",
    [
      { value: "event", label: "Event" },
      { value: "alarm", label: "Alarm" },
    ],
    existing?.kind ?? "event",
    (v) => { typeVal = v; },
  );
  let typeVal: CalendarEvent["kind"] = existing?.kind ?? "event";
  root.append(typeGroup);

  let recurrenceVal: CalendarEvent["recurrence"] = existing?.recurrence ?? "none";
  const recGroup = radioGroup<CalendarEvent["recurrence"]>(
    "Repeat",
    RECURRENCE_OPTIONS,
    existing?.recurrence ?? "none",
    (v) => {
      recurrenceVal = v;
      weekdayWrap.style.display = v === "weekly" ? "" : "none";
    },
  );
  root.append(recGroup);

  root.append(weekdayWrap);

  // Notes — multi-line textarea so users can attach extra info.
  const notesInput = document.createElement("textarea");
  notesInput.className = "zen-textarea";
  notesInput.placeholder = "Notes (optional)";
  notesInput.rows = 3;
  notesInput.style.height = "150px";
  notesInput.value = existing?.notes ?? "";
  root.append(field("Notes", notesInput));

  // Location — free text (address) or a link (URL). When a URL, the
  // events list renders an "open link" button next to edit/delete.
  const locationInput = document.createElement("input");
  locationInput.type = "text";
  locationInput.className = "zen-input";
  locationInput.placeholder = "Location (address or link)";
  locationInput.value = existing?.location ?? "";
  root.append(field("Location", locationInput));

  const read = (): Omit<CalendarEvent, "id" | "created_at" | "updated_at"> | null => {
    const title = titleInput.value.trim();
    if (!title) return null;
    return {
      title,
      date: dateInput.value || new Date().toISOString().slice(0, 10),
      time: timeInput.value || null,
      end_time: null,
      kind: typeVal,
      recurrence: recurrenceVal,
      weekdays: weekdayMask,
      enabled: enabledVal,
      notes: notesInput.value,
      location: locationInput.value.trim(),
      source: "" as CalendarSource,
      source_account_id: "",
      external_id: "",
      notify_on_start: typeVal === "event",
      last_notified_at: 0,
    };
  };

  return { root, read };
}

function field(labelText: string, control: HTMLElement): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "zen-field";
  const label = document.createElement("label");
  label.className = "zen-label";
  label.textContent = labelText;
  wrap.append(label, control);
  return wrap;
}

interface RadioOption<T> {
  value: T;
  label: string;
}

/** Build a `.zen-radio-group` of `.zen-radio-card` tiles (shadcn
 *  radio-cards). Returns a `.zen-field` wrapper with the group
 *  inside. Click a card selects its radio and calls `onChange` with
 *  the chosen value. The selected card gets `.is-selected`. Mirrors
 *  the Repeat control — do NOT inline this elsewhere (AGENTS.md §6). */
function radioGroup<T extends string>(
  labelText: string,
  options: RadioOption<T>[],
  initial: T,
  onChange?: (v: T) => void,
): HTMLElement {
  const fieldWrap = document.createElement("div");
  fieldWrap.className = "zen-field";
  const label = document.createElement("label");
  label.className = "zen-label";
  label.textContent = labelText;
  fieldWrap.append(label);

  const group = document.createElement("div");
  group.className = "zen-radio-group";
  const cards: { value: T; el: HTMLLabelElement }[] = [];

  for (const opt of options) {
    const card = document.createElement("label");
    card.className = "zen-radio-card";
    if (opt.value === initial) card.classList.add("is-selected");
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = `cal-${labelText.toLowerCase().replace(/\s+/g, "-")}`;
    radio.value = opt.value;
    radio.checked = opt.value === initial;
    const text = document.createElement("span");
    text.textContent = opt.label;
    card.append(radio, text);
    card.addEventListener("click", (e) => {
      // Prevent the browser-default double toggle (label + click).
      e.preventDefault();
      for (const c of cards) c.el.classList.toggle("is-selected", c.value === opt.value);
      onChange?.(opt.value);
    });
    cards.push({ value: opt.value, el: card });
    group.append(card);
  }

  fieldWrap.append(group);
  return fieldWrap;
}
