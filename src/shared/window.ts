import { getCurrentWindow } from "@tauri-apps/api/window";

import { applyIcons } from "./icon";
import { loadConfig } from "./config";

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

export async function mountWindow(opts: MountOptions): Promise<MountedWindow> {
  await applyTheme();

  const root = ensureRoot();
  root.className = "zen-window";

  const header = document.createElement("header");
  header.className = "zen-window__header";
  header.dataset.tauriDragRegion = "";

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

  root.replaceChildren(header, content);
  applyIcons(root);

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
