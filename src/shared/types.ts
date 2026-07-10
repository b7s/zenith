export type BackgroundMode = "acrylic" | "mica" | "solid" | "gradient" | "none";
export type ThemeMode = "auto" | "dark" | "light";
export type WidgetZone = "left" | "center" | "right";
export type MotionBackend = "auto" | "gpu" | "cpu";

export interface AppearanceConfig {
  background: BackgroundConfig;
  tint_alpha: number;
  corner_radius: number;
  margin_top: number;
  margin_right: number;
  margin_bottom: number;
  margin_left: number;
  padding_top: number;
  padding_right: number;
  padding_bottom: number;
  padding_left: number;
  bar_height: number;
  theme: ThemeMode;
}

export interface BackgroundConfig {
  mode: BackgroundMode;
  color_top: string;
  color_bottom: string;
  alpha_top: number;
  alpha_bottom: number;
}

export interface LayoutConfig {
  position: "top";
}

export interface WidgetsConfig {
  enabled: string[];
  positions: Record<string, WidgetZone>;
  config: Record<string, Record<string, unknown>>;
}

export interface MotionConfig {
  backend: MotionBackend;
  reduced_motion: boolean;
}

export interface CssConfig {
  custom_enabled: boolean;
}

export interface Config {
  appearance: AppearanceConfig;
  monitors: "all" | string[];
  layout: LayoutConfig;
  widgets: WidgetsConfig;
  motion: MotionConfig;
  css: CssConfig;
}

export interface WidgetManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  default_zone: WidgetZone;
  icon: string;
  min_width: number;
  preview: string;
  widget_dir: string;
  config?: Record<string, WidgetConfigField>;
}

export type WidgetConfigType = "string" | "int" | "bool" | "select";

export interface WidgetConfigField {
  type: WidgetConfigType;
  value: unknown;
  label?: string;
  hint?: string;
  options?: (string | number)[];
}

export interface WidgetSource {
  html: string;
  css: string;
  js: string;
}

export type EventKind = "event" | "alarm";
export type Recurrence = "none" | "daily" | "weekly" | "monthly";

/** Mirrored in `src-tauri/src/events/model.rs::CalendarEvent`. */
export interface CalendarEvent {
  id: string;
  title: string;
  /** ISO-8601 "YYYY-MM-DD". */
  date: string;
  /** "HH:MM" or null for all-day. */
  time: string | null;
  kind: EventKind;
  recurrence: Recurrence;
  /** Weekly recurrence bitmask: bit0=Sun, bit1=Mon, ..., bit6=Sat. */
  weekdays: number;
  enabled: boolean;
  /** Epoch seconds — used for sync conflict resolution. */
  created_at: number;
  updated_at: number;
  /** Free-text notes. */
  notes: string;
}

/** Mirrored in `src-tauri/src/media/mod.rs::MediaInfo`. */
export interface MediaInfo {
  title: string;
  artist: string;
  album: string;
  /** `data:image/...;base64,...` URL or null. */
  thumbnail: string | null;
  /** "playing" | "paused" | "stopped" | "closed" | "opened" | "changing" | "unknown". */
  status: string;
  /** Current position in milliseconds. */
  position_ms: number;
  /** Total duration in milliseconds. */
  duration_ms: number;
  /** Playback rate multiplier (1.0 normal). */
  rate: number;
  /** Source app user model id (e.g. "spotify.exe" / app aumid). */
  source: string;
}

/** Mirrored in `src-tauri/src/media/commands.rs::MediaSnapshot`. */
export interface MediaSnapshot {
  available: boolean;
  info: MediaInfo | null;
}
