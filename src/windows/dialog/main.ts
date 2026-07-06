import { invoke } from "@tauri-apps/api/core";
import { mountDialog } from "./base";
import { getDialogBuilder } from "./registry";
import "./builders";

interface PreInjectedSpec {
  kind: string;
  data: unknown;
}

interface PreInjectedDialogGlobals {
  __zenith_dialog_spec: PreInjectedSpec;
  __ZENITH_DIALOG_KIND: string;
}

void (async () => {
  // The dialog spec is injected via Tauri's `initialization_script` BEFORE the
  // page parses, so it's available synchronously without any IPC roundtrip.
  // Falls back to `get_dialog_data` only if the init script didn't run
  // (e.g. dev mode hot-reload). This avoids a 50-300 ms `await invoke(...)`
  // on every open.
  const pre = (window as unknown as Partial<PreInjectedDialogGlobals>);
  let kind = pre.__ZENITH_DIALOG_KIND ?? pre.__zenith_dialog_spec?.kind ?? "unknown";
  let data: unknown = pre.__zenith_dialog_spec?.data ?? null;

  if (kind === "unknown" || data === null) {
    try {
      const payload = await invoke<[string, unknown]>("get_dialog_data");
      kind = payload[0] ?? kind;
      data = payload[1] ?? data;
    } catch (e) {
      console.error("[dialog] get_dialog_data failed:", e);
    }
  }

  const builder = getDialogBuilder(kind);
  if (!builder) {
    console.error(`[dialog] no builder registered for kind="${kind}"`);
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
    return;
  }

  const opts = builder(data);
  await mountDialog({ ...opts, data });
})();
