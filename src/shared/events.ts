export const EVENT = {
  configUpdated: "zenith:config-updated",
  appearanceChanged: "zenith:appearance-changed",
  widgetsChanged: "zenith:widgets-changed",
  workspaceChanged: "zenith:workspace-changed",
} as const;

export type EventName = (typeof EVENT)[keyof typeof EVENT];
