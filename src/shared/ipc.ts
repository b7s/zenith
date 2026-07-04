export const CMD = {
  getConfig: "get_config",
  saveConfig: "save_config",
  getWidgets: "get_widgets",
  getWidgetSource: "get_widget_source",
} as const;

export type CommandName = (typeof CMD)[keyof typeof CMD];
