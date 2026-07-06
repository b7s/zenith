import type { DialogOptions } from "./base";

type DialogBuilder = (data: unknown) => DialogOptions;

const REGISTRY: Record<string, DialogBuilder> = {};

export function registerDialog(kind: string, builder: DialogBuilder): void {
  REGISTRY[kind] = builder;
}

export function getDialogBuilder(kind: string): DialogBuilder | undefined {
  return REGISTRY[kind];
}