import { getCurrentWindow } from "@tauri-apps/api/window";

import { applyIcons } from "./icon";
import { logInfo } from "./log";

export function isSystemDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

/**
 * Apply the theme synchronously from system preferences and fire a background
 * `loadConfig()` to override with the user's auto/dark/light preference.
 * Returns when the system theme is applied — the user pref may follow a
 * frame later. This avoids an IPC roundtrip (`invoke("get_config")`) BEFORE
 * the dialog/window chrome can paint.
 */
export async function applyTheme(): Promise<"dark" | "light"> {
  const sys = isSystemDark() ? "dark" : "light";
  document.documentElement.dataset.theme = sys;
  // Best-effort refinement from config; if it fails or differs, repaint.
  void (async () => {
    try {
      const { loadConfig } = await import("./config");
      const { theme } = (await loadConfig()).appearance;
      const dark = theme === "dark" || (theme === "auto" && isSystemDark());
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    } catch {
      /* keep system theme */
    }
  })();
  return sys;
}

export function watchSystemTheme(onChange: (dark: boolean) => void): () => void {
  const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
  if (!mq) return () => {};
  const handler = (e: MediaQueryListEvent) => onChange(e.matches);
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}

export interface MountOptions {
  title: string;
  searchable?: boolean;
  searchPlaceholder?: string;
  /** Optional footer element(s) appended below the content. The footer is
   *  rendered as a fixed band outside `<main>`, so action buttons stay
   *  visible when the content scrolls. */
  footer?: HTMLElement | HTMLElement[];
}

export interface MountedWindow {
  root: HTMLElement;
  content: HTMLElement;
  search: HTMLInputElement | null;
  footer: HTMLElement | null;
}

/**
 * Wire the header so `pointerdown` anywhere outside an interactive child
 * starts a window drag. Does NOT use `data-tauri-drag-region` because
 * the declarative approach can swallow click events on transparent windows,
 * making the close button and search field intermittently unresponsive.
 */
function enableDrag(header: HTMLElement): void {
  header.addEventListener("pointerdown", (e) => {
    const target = e.target as HTMLElement;
    if (target.closest("button, input, select, textarea, [data-no-drag]")) return;
    void getCurrentWindow().startDragging();
  });
}

export async function mountWindow(opts: MountOptions): Promise<MountedWindow> {
  const t0 = performance.now();
  await applyTheme();
  logInfo(`mountWindow: applyTheme ${Math.round(performance.now() - t0)}ms`);

  const root = ensureRoot();
  root.className = "zen-window";

  const header = document.createElement("header");
  header.className = "zen-window__header";

  const title = document.createElement("div");
  title.className = "zen-window__title";
  title.textContent = opts.title;
  header.append(title);

  let search: HTMLInputElement | null = null;
  if (opts.searchable) {
    const wrap = document.createElement("div");
    wrap.className = "zen-window__search";

    const sIcon = document.createElement("i");
    sIcon.dataset.icon = "search";
    sIcon.dataset.size = "14";
    sIcon.className = "zen-window__search-icon";
    wrap.append(sIcon);

    search = document.createElement("input");
    search.type = "search";
    search.className = "zen-input zen-window__search-input";
    search.placeholder = opts.searchPlaceholder ?? "Search\u2026";
    search.setAttribute("aria-label", "Search");
    wrap.append(search);

    header.append(wrap);
  }

  const close = document.createElement("button");
  close.className = "zen-icon-button zen-window__close";
  close.dataset.icon = "x";
  close.dataset.size = "16";
  close.setAttribute("aria-label", "Close");
  close.addEventListener("click", () => {
    void getCurrentWindow().close().catch(() => window.close());
  });
  header.append(close);

  const content = document.createElement("main");
  content.className = "zen-window__content";

  const footer = opts.footer ? document.createElement("footer") : null;
  if (footer && opts.footer) {
    footer.className = "zen-window__footer";
    const items: HTMLElement[] = Array.isArray(opts.footer) ? opts.footer : [opts.footer];
    for (const item of items) footer.append(item);
  }

  const t1 = performance.now();
  if (footer) {
    root.replaceChildren(header, content, footer);
  } else {
    root.replaceChildren(header, content);
  }
  enableDrag(header);
  applyIcons(root);
  logInfo(`mountWindow: DOM build + icons ${Math.round(performance.now() - t1)}ms`);

  return { root, content, search, footer };
}

function ensureRoot(): HTMLElement {
  const existing = document.querySelector<HTMLElement>("#root");
  if (existing) return existing;
  const created = document.createElement("div");
  created.id = "root";
  document.body.append(created);
  return created;
}
