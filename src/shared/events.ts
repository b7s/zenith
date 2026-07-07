export const EVENT = {
  configUpdated: "zenith:config-updated",
  appearanceChanged: "zenith:appearance-changed",
  arrangeMode: "zenith:arrange-mode",
  widgetsChanged: "zenith:widgets-changed",
  workspaceChanged: "zenith:workspace-changed",
  workspaceRename: "zenith:workspace-rename",
  workspaceDelete: "zenith:workspace-delete",
  workspaceCreate: "zenith:workspace-create",
  workspaceMoveHere: "zenith:workspace-move-here",
  workspaceMoveTo: "zenith:workspace-move-to",
  workspaceTogglePin: "zenith:workspace-toggle-pin",
  crossDragStart: "zenith:cross-drag-start",
  crossDragMove: "zenith:cross-drag-move",
  crossDragEnd: "zenith:cross-drag-end",
} as const;

export type EventName = (typeof EVENT)[keyof typeof EVENT];

export interface CrossDragPayload {
  id: string;
  x?: number;
  y?: number;
}
