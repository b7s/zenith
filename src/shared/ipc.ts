export const CMD = {
  getConfig: "get_config",
  saveConfig: "save_config",
  getWidgets: "get_widgets",
  getWidgetSource: "get_widget_source",
  openWidgets: "open_widgets",
  getWorkspaces: "get_workspaces",
  getActiveWorkspace: "get_active_workspace",
  switchWorkspace: "switch_workspace",
} as const;

export type CommandName = (typeof CMD)[keyof typeof CMD];
