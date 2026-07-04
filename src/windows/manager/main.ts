import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";
import { initLog, logMemory, logInfo, time } from "../../shared/log";

void (async () => {
  await initLog();
  logMemory("startup");

  const { content, search } = await time("mountWindow", () =>
    mountWindow({ title: "Widgets", searchable: true, searchPlaceholder: "Search widgets" }),
  );

  const hint = document.createElement("p");
  hint.className = "zen-hint";
  hint.textContent = "Widget grid will render here.";
  content.append(hint);

  if (search) {
    search.addEventListener("input", () => {});
  }

  logMemory("after mount");
  logInfo("widgets ready");
})();
