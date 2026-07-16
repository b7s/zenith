import { defineConfig } from "vite";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1422,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1423 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      input: {
        bar: resolve(__dirname, "src/windows/bar/index.html"),
        settings: resolve(__dirname, "src/windows/settings/settings.html"),
        widgets: resolve(__dirname, "src/windows/manager/widgets.html"),
        dialog: resolve(__dirname, "src/windows/dialog/dialog.html"),
        "volume-popup": resolve(__dirname, "widgets/volume/window/volume-popup.html"),
        "widget-config": resolve(__dirname, "src/windows/widget-config/widget-config.html"),
        "calendar": resolve(__dirname, "src/windows/calendar/calendar.html"),
        "shutdown-popup": resolve(__dirname, "widgets/shutdown/window/shutdown-popup.html"),
        "alarm-popup": resolve(__dirname, "widgets/alarms/window/alarm-popup.html"),
        "git-manager": resolve(__dirname, "widgets/git/window/git-manager.html"),
        "webapp-window": resolve(__dirname, "widgets/webapp/window/webapp-window.html"),
        "weather": resolve(__dirname, "widgets/weather/window/weather.html"),
      },
    },
  },
});
