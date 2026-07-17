/**
 * Global right-click guard. Installed once per window (idempotent). When not
 * in dev mode, suppresses the browser/OS context menu everywhere except inside
 * `input` / `textarea` (so users can still use native edit menus). In dev mode
 * the guard is a no-op so the context menu — and devtools — stay available.
 */
let installed = false;

export function installContextMenuGuard(): void {
  if (installed) return;
  installed = true;

  // Dev mode: leave right-click fully enabled (devtools, inspect, etc.).
  if (import.meta.env.DEV) return;

  document.addEventListener("contextmenu", (e) => {
    const target = e.target as HTMLElement | null;
    if (target && target.closest("input, textarea, [data-allow-context]")) return;
    e.preventDefault();
  });
}
