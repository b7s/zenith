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
