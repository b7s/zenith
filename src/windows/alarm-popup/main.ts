import "../../styles/globals.css";
import "./alarm-popup.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { mountWindow } from "../../shared/window";
import { initLog, logInfo } from "../../shared/log";
import { setIcon } from "../../shared/icon";

void (async () => {
  await initLog();
  logInfo("alarm popup ready");

  const injected = window as unknown as {
    __ZENITH_ALARM_TITLE?: string;
    __ZENITH_ALARM_TIME?: string;
    __ZENITH_ALARM_END?: string;
  };
  const fireTime = injected.__ZENITH_ALARM_TIME ?? "";
  const endText = injected.__ZENITH_ALARM_END ?? "";
  const alarmTitle = injected.__ZENITH_ALARM_TITLE;
  if (alarmTitle && alarmTitle.trim()) {
    document.title = alarmTitle;
  }

  const displayTitle = alarmTitle && alarmTitle.trim() ? alarmTitle : "Alarm";
  const { content, root } = await mountWindow({ title: displayTitle });
  void root;
  content.style.cssText =
    "display:flex;flex-direction:column;gap:0.75rem;padding:1rem;height:100%;overflow:hidden;";

  const body = document.createElement("div");
  body.className = "al-pop-body";

  const iconWrap = document.createElement("span");
  iconWrap.className = "al-pop-icon";
  setIcon(iconWrap, "alarm-clock", { size: 28 });
  body.append(iconWrap);

  const title = document.createElement("div");
  title.className = "al-pop-title";
  title.textContent = displayTitle;
  body.append(title);

  const time = document.createElement("div");
  time.className = "al-pop-time";
  time.textContent = endText.trim() ? `${fireTime} → ${endText}` : fireTime;
  body.append(time);

  content.append(body);

  const footer = document.createElement("div");
  footer.className = "al-pop-footer";
  const dismiss = document.createElement("button");
  dismiss.type = "button";
  dismiss.className = "zen-button is-primary";
  dismiss.textContent = "Dismiss";
  dismiss.addEventListener("click", () => {
    void getCurrentWindow().close().catch(() => window.close());
  });
  footer.append(dismiss);
  content.append(footer);

  document.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === "Escape") {
      void getCurrentWindow().close().catch(() => {});
    }
  });

  const win = getCurrentWindow();
  void win.onFocusChanged(({ payload }) => {
    if (payload === false) void win.close().catch(() => {});
  });
})();
