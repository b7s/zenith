import "../../styles/bar-globals.css";
import { applyTheme, watchSystemTheme } from "../../shared/window";
import { applyIcons } from "../../shared/icon";
import { loadConfig } from "../../shared/config";
import { layoutBar } from "../../shared/widgets";
import { invoke } from "@tauri-apps/api/core";
import { initLog, logMemory, logInfo, logError, time } from "../../shared/log";

void (async () => {
  await initLog();
  logMemory("startup");

  await time("applyTheme", () => applyTheme());
  watchSystemTheme(() => void applyTheme());
  applyIcons();

  const bar = document.getElementById("bar");
  if (!bar) {
    logError("bar element not found");
    return;
  }

  bar.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    void invoke("show_context_menu");
  });

  const cfg = await time("loadConfig", () => loadConfig());
  await time("layoutBar", () => layoutBar(bar, cfg));
  logMemory("after layout");
  logInfo("bar ready");
})();
