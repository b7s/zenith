import { invoke } from "@tauri-apps/api/core";

import { CMD } from "./ipc";
import type { Config } from "./types";

export const DEFAULT_CONFIG: Config = {
  appearance: {
    tint_alpha: 102,
    background: {
      mode: "acrylic",
      color_top: "#1a1a1a",
      color_bottom: "#1a1a1a",
      alpha_top: 90,
      alpha_bottom: 90,
    },
    corner_radius_tl: 0,
    corner_radius_tr: 0,
    corner_radius_br: 0,
    corner_radius_bl: 0,
    margin_top: 0,
    margin_right: 0,
    margin_bottom: 0,
    margin_left: 0,
    padding_top: 0,
    padding_right: 8,
    padding_bottom: 0,
    padding_left: 8,
    bar_height: 40,
    theme: "auto",
  },
  monitors: "all",
  layout: { position: "top" },
  widgets: { enabled: ["clock", "workspace"], positions: { clock: "left", workspace: "left" }, config: {} },
  motion: { backend: "auto", reduced_motion: false },
  css: { custom_enabled: true },
  calendar_oauth: { google_client_id: "", outlook_client_id: "" },
  updates: { auto_update: true, start_with_windows: true },
  storage: { onedrive_sync_enabled: false },
};

let cache: Config | null = null;

/**
 * Load the full config. Always resolves to a usable Config:
 * - backend never errors (get_config returns Config::default on any failure)
 * - if the Tauri bridge itself rejects, the TS DEFAULT_CONFIG is returned
 * Results are cached; pass { force: true } to bypass the cache.
 */
export async function loadConfig(opts?: { force?: boolean }): Promise<Config> {
  if (cache && !opts?.force) return cache;
  try {
    const cfg = await invoke<Config>(CMD.getConfig);
    cache = cfg ?? DEFAULT_CONFIG;
  } catch (e) {
    console.error("[zenith] loadConfig failed; using defaults", e);
    cache = DEFAULT_CONFIG;
  }
  return cache;
}

/**
 * Read a single nested value by slash pointer with a fallback.
 * Example: getConfigValue("/appearance/bar_height", 40)
 */
export async function getConfigValue<T>(pointer: string, fallback: T): Promise<T> {
  const cfg = await loadConfig();
  const parts = pointer.split("/").filter(Boolean);
  let cur: unknown = cfg;
  for (const p of parts) {
    if (cur && typeof cur === "object" && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p];
    } else {
      return fallback;
    }
  }
  return (cur as T) ?? fallback;
}

/** Persist config and clear the cache so the next loadConfig() reflects the change. */
export async function saveConfig(config: Config): Promise<void> {
  await invoke(CMD.saveConfig, { config });
  cache = config;
}
