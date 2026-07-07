/**
 * Widget arrangement — single source of truth for arrange-mode state,
 * widget config operations, action buttons, long-press, and bar drop-zones.
 *
 * Both the bar window (`src/windows/bar/main.ts`) and the widget manager
 * (`src/windows/manager/main.ts`) import from this module. No widget
 * manipulation logic should be duplicated in either window's main.ts.
 */
import { emit, listen } from "@tauri-apps/api/event";
import { saveConfig } from "./config";
import { EVENT } from "./events";
import type { CrossDragPayload } from "./events";
import type { Config, WidgetZone } from "./types";

/* --------------------------- arrange-mode state --------------------------- */

let _active = false;
/** When the widget manager is open it "holds" arrange mode so a click on the
 *  bar's empty area or the bar losing focus does NOT deactivate it. */
let _managerHeld = false;
/** Set by the drag system right after a drop so the synthetic `click` that
 *  follows `pointerup` doesn't deactivate arrange mode via the outside-click
 *  handler. Cleared on the next tick. */
let _suppressClick = false;
const _changeListeners = new Set<(active: boolean) => void>();

export function isArrangeActive(): boolean {
  return _active;
}

/** Toggle arrange mode on/off. `broadcast=true` (default) emits the event so
 *  the other window syncs. `broadcast=false` is used by the event listener to
 *  avoid an infinite emit loop. */
export function setArrangeActive(active: boolean, broadcast = true): void {
  if (active === _active) return;
  _active = active;
  document.body.classList.toggle("is-arranging", active);
  for (const fn of _changeListeners) fn(active);
  if (broadcast) {
    void emit(EVENT.arrangeMode, { active });
  }
}

/** The widget manager calls this on open so arrange mode stays on while the
 *  manager window is visible, regardless of bar clicks/focus. Pair with
 *  `releaseArrangeHold()` on manager close. */
export function holdArrange(): void {
  _managerHeld = true;
  setArrangeActive(true);
}

/** Manager close: release the hold and deactivate (unless long-press re-armed). */
export function releaseArrangeHold(): void {
  _managerHeld = false;
  setArrangeActive(false);
}

export function toggleArrangeMode(): boolean {
  setArrangeActive(!_active);
  return _active;
}

export function onArrangeChange(fn: (active: boolean) => void): () => void {
  _changeListeners.add(fn);
  return () => {
    _changeListeners.delete(fn);
  };
}

/** Wire up cross-window sync. Returns an unlisten function. Call once per
 *  window in the entry point. */
export function initArrangeSync(): Promise<() => void> {
  return listen<{ active: boolean }>(EVENT.arrangeMode, (e) => {
    setArrangeActive(e.payload.active, false);
  });
}

/** Deactivate arrange mode on a click outside any widget — but only if the
 *  widget manager is NOT holding it. Call once from the bar's entry point.
 *  A click is "outside" if it did not hit a `.widget-slot` or `.zen-widget-btn`.
 *  Also deactivates when the bar window itself loses focus (clicking another
 *  app / the desktop), again only if the manager isn't holding arrange. */
export function attachOutsideClickDeactivate(): () => void {
  const onClick = (e: MouseEvent) => {
    if (!_active || _managerHeld) return;
    if (_suppressClick) {
      _suppressClick = false;
      return;
    }
    const t = e.target as HTMLElement | null;
    if (!t) return;
    const onSlot = t.closest(".widget-slot");
    const onBtn = t.closest(".zen-widget-btn");
    if (onSlot || onBtn) return;
    setArrangeActive(false);
  };
  const onBlur = () => {
    if (_active && !_managerHeld) setArrangeActive(false);
  };
  document.addEventListener("click", onClick, true);
  window.addEventListener("blur", onBlur);
  return () => {
    document.removeEventListener("click", onClick, true);
    window.removeEventListener("blur", onBlur);
  };
}

/* ------------------------------ config ops -------------------------------- */

/** Add a widget to the bar (idempotent). Persists + emits `config-updated`,
 *  which the bar listens for and re-lays-out. */
export async function addWidget(cfg: Config, id: string, zone?: WidgetZone): Promise<void> {
  if (cfg.widgets.enabled.includes(id)) return;
  cfg.widgets.enabled.push(id);
  if (zone) cfg.widgets.positions[id] = zone;
  await saveConfig(cfg);
}

/** Remove a widget from the bar. */
export async function removeWidget(cfg: Config, id: string): Promise<void> {
  cfg.widgets.enabled = cfg.widgets.enabled.filter((w) => w !== id);
  await saveConfig(cfg);
}

/** Move an already-enabled widget to a zone and re-order the `enabled` list so
 *  it lands right after the last widget currently in that zone. */
export async function moveWidget(cfg: Config, id: string, zone: WidgetZone): Promise<void> {
  cfg.widgets.positions[id] = zone;
  const rest = cfg.widgets.enabled.filter((w) => w !== id);
  let insertAt = rest.length;
  for (let i = rest.length - 1; i >= 0; i--) {
    if (cfg.widgets.positions[rest[i]] === zone) {
      insertAt = i + 1;
      break;
    }
  }
  rest.splice(insertAt, 0, id);
  cfg.widgets.enabled = rest;
  await saveConfig(cfg);
}

/* --------------------------- action button ------------------------------- */

/** Round green "+"/red "−" button appended to a widget slot/card. The handler
 *  is responsible for calling addWidget/removeWidget. */
export function createWidgetActionBtn(
  type: "add" | "remove",
  handler: () => void,
): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = `zen-widget-btn is-${type}`;
  btn.textContent = type === "add" ? "+" : "\u2212";
  btn.title = type === "add" ? "Add to bar" : "Remove from bar";
  btn.setAttribute("aria-label", btn.title);
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    e.preventDefault();
    handler();
  });
  return btn;
}

/* ------------------------------ long press -------------------------------- */

const DEFAULT_HOLD_MS = 550;

/** Attach a long-press recognizer to `el`. Returns a detach function. Uses
 *  pointer events so it works for both mouse and touch. The callback fires
 *  only if the pointer stays down for `ms` without moving or leaving — this
 *  is critical: without the move cancel, the timer fires mid-drag (after ~550ms)
 *  and toggles arrange mode OFF while the user is still dragging a widget,
 *  which instantly blocks the drop. */
export function attachLongPress(el: HTMLElement, cb: () => void, ms = DEFAULT_HOLD_MS): () => void {
  let timer: number | undefined;
  const start = () => {
    timer = window.setTimeout(() => {
      timer = undefined;
      cb();
    }, ms);
  };
  const cancel = () => {
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timer = undefined;
    }
  };
  el.addEventListener("pointerdown", start);
  el.addEventListener("pointerup", cancel);
  el.addEventListener("pointerleave", cancel);
  el.addEventListener("pointercancel", cancel);
  el.addEventListener("pointermove", cancel);
  return () => {
    el.removeEventListener("pointerdown", start);
    el.removeEventListener("pointerup", cancel);
    el.removeEventListener("pointerleave", cancel);
    el.removeEventListener("pointercancel", cancel);
    el.removeEventListener("pointermove", cancel);
    cancel();
  };
}

/* --------------------- bar: arrange UI + drag system --------------------- */

/** Re-apply arrange-mode chrome (action buttons) to every `.widget-slot`
 *  under `bar`. Idempotent — safe to call after every layout. Pass the live
 *  `cfg` so the remove handler captures the current config. */
export function applyArrangeUI(bar: HTMLElement, cfg: Config): void {
  const active = _active;
  const slots = bar.querySelectorAll<HTMLElement>(".widget-slot");
  for (const slot of slots) {
    slot.querySelector(".zen-widget-btn")?.remove();
    if (!active) continue;
    const id = slot.dataset.widgetId;
    if (!id) continue;
    slot.append(
      createWidgetActionBtn("remove", () => {
        void removeWidget(cfg, id);
      }),
    );
  }
}

/** Wire pointer-based sortable drag-and-drop for the bar.
 *
 *  Implements SortableJS-style live reordering: as the user drags a widget
 *  over others, they physically swap positions in real-time with a smooth
 *  FLIP animation. On release, the new DOM order is read back into config.
 *
 *  We do NOT use HTML5 DnD because WebView2 in transparent Tauri windows
 *  kills the drag operation immediately after `dragstart`. */
export function setupBarDropZones(bar: HTMLElement, cfg: Config): () => void {
  let dragSlot: HTMLElement | null = null;

  /** FLIP animation: record positions → mutate DOM → apply inverse transform
   *  → animate to zero on next frame. Only animates non-dragged slots. */
  const flipMove = (mutate: () => void): void => {
    const slots = [...bar.querySelectorAll<HTMLElement>(".widget-slot:not(.is-dragging)")];
    const before = new Map(slots.map((s) => [s, s.getBoundingClientRect()]));
    mutate();
    let needsPlay = false;
    for (const s of slots) {
      const b = before.get(s)!;
      const a = s.getBoundingClientRect();
      const dx = b.left - a.left;
      if (dx === 0) continue;
      s.style.transition = "none";
      s.style.transform = `translateX(${dx}px)`;
      needsPlay = true;
    }
    if (!needsPlay) return;
    requestAnimationFrame(() => {
      for (const s of slots) {
        if (s.style.transform) {
          s.style.transition = "transform 130ms ease";
          s.style.transform = "";
        }
      }
    });
    setTimeout(() => {
      for (const s of slots) {
        s.style.transition = "";
        s.style.transform = "";
      }
    }, 180);
  };

  /** Find the zone element at the given x-coordinate. */
  const zoneAtX = (x: number): HTMLElement | null => {
    for (const z of bar.querySelectorAll<HTMLElement>(".bar-zone")) {
      const r = z.getBoundingClientRect();
      if (x >= r.left && x <= r.right) return z;
    }
    return null;
  };

  /** Within a zone, find the slot whose midpoint is to the right of x.
   *  The dragged slot should be inserted BEFORE this slot. Returns null if
   *  x is past every slot's midpoint (= append at end). */
  const insertionTarget = (
    zone: HTMLElement,
    x: number,
    exclude: HTMLElement,
  ): HTMLElement | null => {
    for (const s of zone.querySelectorAll<HTMLElement>(".widget-slot")) {
      if (s === exclude) continue;
      const r = s.getBoundingClientRect();
      if (x < r.left + r.width / 2) return s;
    }
    return null;
  };

  /** Check if draggedSlot is already in the right position — avoids
   *  unnecessary DOM moves and animation on every pointermove tick. */
  const alreadyInPosition = (
    zone: HTMLElement,
    before: HTMLElement | null,
  ): boolean => {
    if (dragSlot!.parentElement !== zone) return false;
    if (before && dragSlot!.nextElementSibling === before) return true;
    if (!before && zone.lastElementChild === dragSlot) return true;
    return false;
  };

  const onPointerDown = (e: PointerEvent) => {
    if (!_active) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest(".zen-widget-btn")) return;
    const slot = target?.closest<HTMLElement>(".widget-slot");
    if (!slot?.dataset.widgetId) return;
    dragSlot = slot;
    slot.classList.add("is-dragging");
    document.body.classList.add("zen-dragging");
  };

  const onPointerMove = (e: PointerEvent) => {
    if (!dragSlot) return;
    const zone = zoneAtX(e.clientX);
    if (!zone) return;
    const before = insertionTarget(zone, e.clientX, dragSlot);
    if (alreadyInPosition(zone, before)) return;
    flipMove(() => {
      if (before) zone.insertBefore(dragSlot!, before);
      else zone.appendChild(dragSlot!);
    });
  };

  const onPointerUp = () => {
    if (!dragSlot) return;
    dragSlot.classList.remove("is-dragging");
    document.body.classList.remove("zen-dragging");

    // Read new order from DOM → config
    const enabled: string[] = [];
    const positions: Record<string, string> = {};
    for (const zone of bar.querySelectorAll<HTMLElement>(".bar-zone")) {
      const zn = zone.dataset.barZone!;
      for (const slot of zone.querySelectorAll<HTMLElement>(".widget-slot")) {
        const id = slot.dataset.widgetId!;
        enabled.push(id);
        positions[id] = zn;
      }
    }
    cfg.widgets.enabled = enabled;
    cfg.widgets.positions = positions as Record<string, WidgetZone>;
    void saveConfig(cfg);

    _suppressClick = true;
    setTimeout(() => { _suppressClick = false; }, 50);
    dragSlot = null;
  };

  bar.addEventListener("pointerdown", onPointerDown);
  document.addEventListener("pointermove", onPointerMove);
  document.addEventListener("pointerup", onPointerUp);
  document.addEventListener("pointercancel", onPointerUp);

  return () => {
    bar.removeEventListener("pointerdown", onPointerDown);
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
    document.removeEventListener("pointercancel", onPointerUp);
  };
}

/* ----------------- cross-window drag: manager → bar ----------------------- */

const DRAG_THRESHOLD = 6;
let _crossActive = false;

/** Manager side: make a widget card draggable toward the bar.
 *  A "faked" ghost chip follows the cursor inside the manager window while
 *  the bar (synced via `zenith:cross-drag-start` / `-end`) shows zone drop
 *  indicators. On `pointerup` over the bar, the bar adds the real widget.
 *  Only attach to cards for widgets NOT already enabled on the bar. */
export function attachCrossDragSender(card: HTMLElement, id: string): () => void {
  let startX = 0;
  let startY = 0;
  let ghost: HTMLElement | null = null;
  let pid = -1;

  const cleanup = () => {
    if (ghost) {
      ghost.remove();
      ghost = null;
    }
    _crossActive = false;
    document.body.classList.remove("zen-cross-dragging");
    void emit(EVENT.crossDragEnd, { id } satisfies CrossDragPayload);
  };

  const onDown = (e: PointerEvent) => {
    if (_crossActive) return;
    const t = e.target as HTMLElement | null;
    if (t?.closest(".zen-widget-btn")) return;
    pid = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
  };

  const onMove = (e: PointerEvent) => {
    if (pid !== e.pointerId) return;
    if (!_crossActive) {
      const dx = e.clientX - startX;
      const dy = e.clientY - startY;
      if (dx * dx + dy * dy < DRAG_THRESHOLD * DRAG_THRESHOLD) return;
      _crossActive = true;
      document.body.classList.add("zen-cross-dragging");
      ghost = document.createElement("div");
      ghost.className = "zen-cross-ghost";
      const label = card.querySelector(".widget-card__name")?.textContent || id;
      ghost.textContent = label;
      document.body.append(ghost);
      void emit(EVENT.crossDragStart, { id } satisfies CrossDragPayload);
    }
    if (ghost) {
      ghost.style.left = `${e.clientX}px`;
      ghost.style.top = `${e.clientY}px`;
    }
  };

  const onUp = (e: PointerEvent) => {
    if (pid !== e.pointerId) return;
    pid = -1;
    if (_crossActive) cleanup();
  };

  card.addEventListener("pointerdown", onDown);
  document.addEventListener("pointermove", onMove);
  document.addEventListener("pointerup", onUp);
  document.addEventListener("pointercancel", onUp);

  return () => {
    card.removeEventListener("pointerdown", onDown);
    document.removeEventListener("pointermove", onMove);
    document.removeEventListener("pointerup", onUp);
    document.removeEventListener("pointercancel", onUp);
    if (_crossActive) cleanup();
  };
}

/** Bar side: listen for cross-window drag and show zone drop indicators.
 *  On `pointerup` over a zone, add the dragged widget to that zone. */
export function setupBarReceiveDrop(bar: HTMLElement, cfg: Config): () => void {
  let dragId: string | null = null;

  const clearZones = () => {
    for (const z of bar.querySelectorAll<HTMLElement>(".bar-zone")) {
      z.classList.remove("is-drop-target");
    }
  };

  const endReceiving = () => {
    if (dragId === null) return;
    dragId = null;
    bar.classList.remove("is-receiving");
    clearZones();
  };

  const zoneAtX = (x: number): HTMLElement | null => {
    for (const z of bar.querySelectorAll<HTMLElement>(".bar-zone")) {
      const r = z.getBoundingClientRect();
      if (x >= r.left && x <= r.right) return z;
    }
    return null;
  };

  const onMove = (e: PointerEvent) => {
    if (dragId === null) return;
    clearZones();
    const z = zoneAtX(e.clientX);
    if (z) z.classList.add("is-drop-target");
  };

  const onUp = (e: PointerEvent) => {
    if (dragId === null) return;
    const z = zoneAtX(e.clientX);
    const id = dragId;
    endReceiving();
    if (z && z.dataset.barZone) {
      void addWidget(cfg, id, z.dataset.barZone as WidgetZone);
    }
  };

  bar.addEventListener("pointermove", onMove);
  bar.addEventListener("pointerup", onUp);

  const unlistenStart = listen<CrossDragPayload>(EVENT.crossDragStart, (e) => {
    if (dragId !== null) return;
    dragId = e.payload.id;
    bar.classList.add("is-receiving");
  });
  const unlistenEnd = listen(EVENT.crossDragEnd, () => {
    endReceiving();
  });

  return () => {
    bar.removeEventListener("pointermove", onMove);
    bar.removeEventListener("pointerup", onUp);
    void unlistenStart.then((f) => f());
    void unlistenEnd.then((f) => f());
  };
}
