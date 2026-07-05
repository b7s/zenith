export const CMD = {
  getConfig: "get_config",
  saveConfig: "save_config",
  getWidgets: "get_widgets",
  getWidgetSource: "get_widget_source",
  openWidgets: "open_widgets",
  getWorkspaces: "get_workspaces",
  getActiveWorkspace: "get_active_workspace",
  switchWorkspace: "switch_workspace",
  moveWindowToDesktop: "move_window_to_desktop",
  createDesktop: "create_desktop",
  deleteDesktop: "delete_desktop",
  renameDesktop: "rename_desktop",
  togglePinWindow: "toggle_pin_window",
  showWorkspaceContextMenu: "show_workspace_context_menu",
  confirmDeleteDesktop: "confirm_delete_desktop",
  showRenameDialog: "show_rename_dialog",
  getRenameData: "get_rename_data",
} as const;

export type CommandName = (typeof CMD)[keyof typeof CMD];
