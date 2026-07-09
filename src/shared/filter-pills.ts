export interface FilterPillDef<T extends string> {
  id: T;
  label: string;
}

export interface FilterPillsMount<T extends string> {
  container: HTMLElement;
  readonly activeId: T;
  switchTo(id: T): void;
}

/**
 * Horizontal segmented control of pill buttons exactly one of which is
 * "active" at a time. Reusable across windows that need a 3-up All/X/Y
 * filter above a list (calendar events, future filterable widgets, the
 * log viewer, etc.). Renders nothing but `.zen-filter-pills` +
 * `.zen-filter-pill` so the visual contract lives in `components.css`.
 *
 * Use the controlled signature when the parent already owns the state
 * (preferred — same pattern as `mountTabs`): own `let activeId`,
 * re-render with the new pill on change. The `switchTo` helper only
 * toggles the `is-active` classes; the caller decides what to do with
 * the id (filter, navigate, etc.).
 *
 *   let mode: "all" | "event" | "alarm" = "all";
 *   const pills = mountFilterPills(parent, [
 *     { id: "all",   label: "All" },
 *     { id: "event", label: "Event" },
 *     { id: "alarm", label: "Alarm" },
 *   ], "all");
 *   pills.switchTo = (id) => { mode = id; render(); };  // not used — keep `tab` style
 *
 * If you want a fully self-contained mount that fires `onChange`, just
 * read `pills.activeId` inside the click handler you pass via the
 * returned `switchTo`. Most callers prefer the controlled pattern.
 */
export function mountFilterPills<T extends string>(
  parent: HTMLElement,
  pills: readonly FilterPillDef<T>[],
  initialId?: T,
): FilterPillsMount<T> {
  const bar = document.createElement("div");
  bar.className = "zen-filter-pills";
  bar.setAttribute("role", "tablist");

  const buttons: Record<string, HTMLButtonElement> = {};
  let activeId: T = initialId ?? pills[0]?.id ?? ("" as T);

  for (const def of pills) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "zen-filter-pill";
    btn.setAttribute("role", "tab");
    btn.setAttribute("aria-selected", "false");
    btn.textContent = def.label;
    btn.dataset.pillId = def.id;
    bar.append(btn);
    buttons[def.id] = btn;
  }

  function setActive(id: T): void {
    if (id === activeId || !buttons[id]) return;
    if (buttons[activeId]) {
      buttons[activeId].classList.remove("is-active");
      buttons[activeId].setAttribute("aria-selected", "false");
    }
    activeId = id;
    if (buttons[activeId]) {
      buttons[activeId].classList.add("is-active");
      buttons[activeId].setAttribute("aria-selected", "true");
    }
  }

  if (activeId && buttons[activeId]) {
    buttons[activeId].classList.add("is-active");
    buttons[activeId].setAttribute("aria-selected", "true");
  }

  bar.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-pill-id]");
    if (btn) setActive(btn.dataset.pillId as T);
  });

  parent.append(bar);
  return {
    container: bar,
    get activeId() { return activeId; },
    switchTo: setActive,
  };
}
