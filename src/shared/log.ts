import { invoke } from "@tauri-apps/api/core";

export type LogLevel = "debug" | "info" | "warn" | "error";

function windowLabel(): string {
  return document.body.dataset.window ?? "unknown";
}

export function log(level: LogLevel, message: string): void {
  void invoke("log_write", { window: windowLabel(), level, message }).catch(() => {});
}

export function logDebug(message: string): void {
  log("debug", message);
}

export function logInfo(message: string): void {
  log("info", message);
}

export function logWarn(message: string): void {
  log("warn", message);
}

export function logError(message: string): void {
  log("error", message);
}

interface PerfMemory {
  usedJSHeapSize: number;
  totalJSHeapSize: number;
  jsHeapSizeLimit: number;
}

export function logMemory(label: string): void {
  const m = (performance as unknown as { memory?: PerfMemory }).memory;
  if (m) {
    const used = Math.round(m.usedJSHeapSize / 1024);
    const total = Math.round(m.totalJSHeapSize / 1024);
    const limit = Math.round(m.jsHeapSizeLimit / 1024 / 1024);
    logInfo(`${label}: JS heap ${used}KB / ${total}KB (limit ${limit}MB)`);
  } else {
    logInfo(`${label}: performance.memory unavailable`);
  }
}

export async function time<T>(label: string, fn: () => Promise<T>): Promise<T> {
  const start = performance.now();
  try {
    const result = await fn();
    const elapsed = (performance.now() - start).toFixed(1);
    logInfo(`${label}: ${elapsed}ms`);
    return result;
  } catch (e) {
    logError(`${label} failed: ${String(e)}`);
    throw e;
  }
}

export async function initLog(): Promise<void> {
  try {
    await invoke("log_clear", { window: windowLabel() });
  } catch {
    /* backend might not be ready during dev */
  }
  logInfo(`--- ${windowLabel()} window starting ---`);
}
