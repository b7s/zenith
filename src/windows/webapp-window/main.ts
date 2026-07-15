import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";
import { initLog } from "../../shared/log";

const linkUrl =
  (window as unknown as { __ZENITH_LINK_URL?: string }).__ZENITH_LINK_URL;

if (linkUrl) {
  window.location.replace(linkUrl);
} else {
  void (async () => {
    await initLog();
    const title =
      (window as unknown as { __ZENITH_LINK_TITLE?: string })
        .__ZENITH_LINK_TITLE || "Link";
    await mountWindow({ title });
  })();
}
