import { getCurrentWindow } from "@tauri-apps/api/window";

import { applyIcons } from "./icon";
import { loadConfig } from "./config";
import { logInfo } from "./log";

export function isSystemDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

export async function applyTheme(): Promise<"dark" | "light"> {
  let dark = isSystemDark();
  try {
    const { theme } = (await loadConfig()).appearance;
    dark = theme === "dark" || (theme === "auto" && isSystemDark());
  } catch {
    /* fall back to system theme */
  }
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  return dark ? "dark" : "light";
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
}

export interface MountedWindow {
  root: HTMLElement;
  content: HTMLElement;
  search: HTMLInputElement | null;
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
    search = document.createElement("input");
    search.type = "search";
    search.className = "zen-input zen-window__search";
    search.placeholder = opts.searchPlaceholder ?? "Search…";
    search.setAttribute("aria-label", "Search");
    header.append(search);
  }

  const spacer = document.createElement("div");
  spacer.className = "zen-window__spacer";
  header.append(spacer);

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

  const t1 = performance.now();
  root.replaceChildren(header, content);
  enableDrag(header);
  applyIcons(root);
  logInfo(`mountWindow: DOM build + icons ${Math.round(performance.now() - t1)}ms`);

  return { root, content, search };
}

function ensureRoot(): HTMLElement {
  const existing = document.querySelector<HTMLElement>("#root");
  if (existing) return existing;
  const created = document.createElement("div");
  created.id = "root";
  document.body.append(created);
  return created;
}
