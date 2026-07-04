import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";

void mountWindow({ title: "Settings" }).then(({ content }) => {
  const hint = document.createElement("p");
  hint.className = "zen-hint";
  hint.textContent = "Appearance editor will render here.";
  content.append(hint);
});
