export const CMD = {
  getConfig: "get_config",
  saveConfig: "save_config",
} as const;

export type CommandName = (typeof CMD)[keyof typeof CMD];
