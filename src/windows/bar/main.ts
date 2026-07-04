import "../../styles/globals.css";
import { applyTheme, watchSystemTheme } from "../../shared/window";
import { applyIcons } from "../../shared/icon";

void applyTheme().then(() => {
  watchSystemTheme(() => void applyTheme());
});

applyIcons();
