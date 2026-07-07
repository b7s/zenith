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
        bar: resolve(__dirname, "index.html"),
        settings: resolve(__dirname, "settings.html"),
        widgets: resolve(__dirname, "widgets.html"),
        dialog: resolve(__dirname, "dialog.html"),
        "volume-popup": resolve(__dirname, "volume-popup.html"),
        "widget-config": resolve(__dirname, "widget-config.html"),
      },
    },
  },
});
