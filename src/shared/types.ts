export type Material = "acrylic" | "mica" | "none";
export type ThemeMode = "auto" | "dark" | "light";
export type BackgroundMode = "transparent" | "solid" | "gradient";
export type GradientDirection = "to_bottom" | "to_top";
export type WidgetZone = "left" | "center" | "right";
export type MotionBackend = "auto" | "gpu" | "cpu";

export interface AppearanceConfig {
  material: Material;
  tint_alpha: number;
  background: BackgroundConfig;
  corner_radius: number;
  margin_top: number;
  margin_left: number;
  margin_right: number;
  bar_height: number;
  theme: ThemeMode;
}

export interface BackgroundConfig {
  mode: BackgroundMode;
  color_top: string;
  color_bottom: string;
  gradient_direction: GradientDirection;
  alpha_top: number;
  alpha_bottom: number;
}

export interface LayoutConfig {
  position: "top";
}

export interface WidgetsConfig {
  enabled: string[];
  positions: Record<string, WidgetZone>;
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
