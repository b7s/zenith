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
 * filter above a list (calendar events, the git widget-config provider
 * filter, the git-manager account filter, etc.). Renders nothing but
 * `.zen-filter-pills` + `.zen-filter-pill` so the visual contract lives in
 * `components.css`.
 *
 * Overflow handling (global, automatic): when the pills don't fit the width
 * of the container's parent, the rightmost non-active pills collapse into a
 * "More ▾" dropdown anchored to the pill bar. Selecting an item from the
 * dropdown (or resizing the window) re-distributes the pills so the active
 * one is always visible — no content is ever lost. See §6.2 in AGENTS.md.
 *
 * Use the controlled signature when the parent already owns the state
 * (preferred — same pattern as `mountTabs`): own `let activeId`,
 * re-render with the new pill on change. The `switchTo` helper only
 * toggles the `is-active` classes; the caller decides what to do with
 * the id (filter, navigate, etc.).
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

  // "More" overflow button + its dropdown (both children of the bar so clicks
  // bubble through the bar's delegated handler; the menu is absolutely
  // positioned so it never affects the bar's measured width).
  const moreBtn = document.createElement("button");
  moreBtn.type = "button";
  moreBtn.className = "zen-filter-pill zen-filter-pill-more";
  moreBtn.textContent = "More ▾";
  moreBtn.style.display = "none";
  moreBtn.setAttribute("aria-haspopup", "true");
  moreBtn.setAttribute("aria-expanded", "false");
  bar.append(moreBtn);

  const moreMenu = document.createElement("div");
  moreMenu.className = "zen-filter-pills__menu";
  moreMenu.style.display = "none";
  moreMenu.setAttribute("role", "menu");
  bar.append(moreMenu);

  function setActive(id: T): void {
    if (id === activeId) return;
    if (buttons[activeId]) {
      buttons[activeId].classList.remove("is-active");
      buttons[activeId].setAttribute("aria-selected", "false");
    }
    activeId = id;
    if (buttons[activeId]) {
      buttons[activeId].classList.add("is-active");
      buttons[activeId].setAttribute("aria-selected", "true");
    }
    reflow();
  }

  if (activeId && buttons[activeId]) {
    buttons[activeId].classList.add("is-active");
    buttons[activeId].setAttribute("aria-selected", "true");
  }

  bar.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-pill-id]");
    if (btn) setActive(btn.dataset.pillId as T);
  });

  moreBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    toggleMenu();
  });
  moreMenu.addEventListener("click", () => {
    closeMenu();
    reflow();
  });

  function visiblePills(): HTMLElement[] {
    return Array.from(bar.children).filter(
      (el): el is HTMLElement =>
        el !== moreBtn &&
        el !== moreMenu &&
        el.classList.contains("zen-filter-pill"),
    );
  }

  function openMenu(): void {
    moreMenu.style.display = "";
    moreBtn.setAttribute("aria-expanded", "true");
    document.addEventListener("click", onDocClick, true);
  }
  function closeMenu(): void {
    moreMenu.style.display = "none";
    moreBtn.setAttribute("aria-expanded", "false");
    document.removeEventListener("click", onDocClick, true);
  }
  function toggleMenu(): void {
    if (moreMenu.style.display === "none") openMenu();
    else closeMenu();
  }
  function onDocClick(e: MouseEvent): void {
    if (!bar.contains(e.target as Node)) closeMenu();
  }

  /** Recompute which pills fit; move the rest into the More dropdown. */
  function reflow(): void {
    if (bar.offsetParent === null) return; // not laid out yet
    // Restore any collapsed pills so we measure the natural width.
    while (moreMenu.firstChild) {
      bar.insertBefore(moreMenu.firstChild, moreBtn);
    }
    closeMenu();

    const avail = parent.clientWidth;
    if (avail <= 0) return;

    moreBtn.style.display = "none";
    const natural = bar.scrollWidth;
    if (natural <= avail) return; // everything fits

    // Overflow: show the More button and collapse excess pills into it.
    moreBtn.style.display = "";
    const moreW = moreBtn.offsetWidth;
    const pills = visiblePills();
    const overflowing: HTMLElement[] = [];
    let used = 0;
    for (const p of pills) {
      const w = p.offsetWidth;
      if (p.dataset.pillId === activeId) {
        used += w; // keep active visible, always
        continue;
      }
      if (used + w + moreW <= avail) used += w;
      else overflowing.push(p);
    }
    for (const p of overflowing) moreMenu.append(p);
  }

  // Run once after layout, and whenever the bar's box changes size
  // (window resize, or dynamic pills added/removed by the caller).
  requestAnimationFrame(() => reflow());
  if (typeof ResizeObserver !== "undefined") {
    new ResizeObserver(() => reflow()).observe(bar);
  }

  parent.append(bar);
  return {
    container: bar,
    get activeId() {
      return activeId;
    },
    switchTo: setActive,
  };
}
