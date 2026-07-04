import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";

void mountWindow({ title: "Widgets", searchable: true, searchPlaceholder: "Search widgets" }).then(
  ({ content, search }) => {
    const hint = document.createElement("p");
    hint.className = "zen-hint";
    hint.textContent = "Widget grid will render here.";
    content.append(hint);

    if (search) {
      search.addEventListener("input", () => {
        // Widget filtering will be wired in the widgets domain.
      });
    }
  },
);
