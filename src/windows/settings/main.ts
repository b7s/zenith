import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";
import { initLog, logMemory, logInfo, time } from "../../shared/log";

void (async () => {
  await initLog();
  logMemory("startup");

  const { content } = await time("mountWindow", () => mountWindow({ title: "Settings" }));

  const hint = document.createElement("p");
  hint.className = "zen-hint";
  hint.textContent = "Appearance editor will render here.";
  content.append(hint);

  logMemory("after mount");
  logInfo("settings ready");
})();
