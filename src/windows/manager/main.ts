import "../../styles/globals.css";
import { mountWindow } from "../../shared/window";
import { loadConfig } from "../../shared/config";
import { getWidgets, getWidgetSource, renderWidget } from "../../shared/widgets";
import { applyIcons } from "../../shared/icon";
import { invoke } from "@tauri-apps/api/core";
import { initLog, logMemory, logInfo, time } from "../../shared/log";
import { listen } from "@tauri-apps/api/event";
import {
  holdArrange,
  releaseArrangeHold,
  initArrangeSync,
  addWidget,
  removeWidget,
  createWidgetActionBtn,
  attachCrossDragSender,
} from "../../shared/widget-arrange";
import { EVENT } from "../../shared/events";
import type { Config, WidgetManifest } from "../../shared/types";

void (async () => {
  await initLog();
  logMemory("startup");

  const { content, search, titleBadge } = await time("mountWindow", () =>
    mountWindow({ title: "Widgets", searchable: true, searchPlaceholder: "Search widgets" }),
  );

  let cfg = await loadConfig();
  let manifests = await getWidgets();
  if (titleBadge) titleBadge.textContent = String(manifests.length);
  let filter = "";

  void initArrangeSync();
  holdArrange();
  window.addEventListener("beforeunload", () => releaseArrangeHold());

  function render(): void {
    content.replaceChildren(buildGrid());
  }

  function buildGrid(): HTMLElement {
    const grid = document.createElement("div");
    grid.className = "widget-grid";

    const q = filter.trim().toLowerCase();
    const shown = manifests.filter(
      (m) => !q || m.name.toLowerCase().includes(q) || m.id.toLowerCase().includes(q),
    );

    for (const m of shown) {
      grid.append(buildCard(m, cfg.widgets.enabled.includes(m.id)));
    }

    if (!shown.length) {
      const empty = document.createElement("p");
      empty.className = "zen-hint";
      empty.style.padding = "1rem";
      empty.textContent = "No widgets match your search.";
      grid.append(empty);
    }
    return grid;
  }

  function buildCard(m: WidgetManifest, enabled: boolean): HTMLElement {
    const card = document.createElement("div");
    card.className = "widget-card";
    if (!enabled) card.classList.add("is-draggable");
    card.dataset.widgetId = m.id;

    const preview = document.createElement("div");
    preview.className = "widget-card__preview";
    card.append(preview);

    const body = document.createElement("div");
    body.className = "widget-card__body";
    const name = document.createElement("div");
    name.className = "widget-card__name";
    name.textContent = m.name;
    const desc = document.createElement("div");
    desc.className = "widget-card__desc";
    desc.textContent = m.description || m.id;
    body.append(name, desc);
    card.append(body);

    card.append(
      createWidgetActionBtn(enabled ? "remove" : "add", () => {
        const op = enabled ? removeWidget(cfg, m.id) : addWidget(cfg, m.id);
        void op;
      }),
    );

    if (m.config && Object.keys(m.config).length > 0) {
      const gearBtn = document.createElement("button");
      gearBtn.type = "button";
      gearBtn.className = "widget-config-btn";
      gearBtn.style.cssText =
        "position:absolute;bottom:6px;right:6px;width:22px;height:22px;" +
        "border-radius:50%;border:none;cursor:pointer;display:flex;" +
        "align-items:center;justify-content:center;color:var(--muted-foreground);" +
        "background:color-mix(in oklch,var(--card) 80%,transparent);";
      gearBtn.title = "Configure";
      gearBtn.dataset.icon = "settings";
      gearBtn.dataset.size = "12";
      gearBtn.addEventListener("click", () => {
        void invoke("open_widget_config", { widgetId: m.id });
      });
      card.style.position = "relative";
      card.append(gearBtn);
      applyIcons(gearBtn);
    }

    // Load real widget HTML into preview area (async — renders live preview)
    void loadPreview(preview, m);

    if (!enabled) attachCrossDragSender(card, m.id);

    return card;
  }

  async function loadPreview(container: HTMLElement, m: WidgetManifest): Promise<void> {
    const source = await getWidgetSource(m.id);
    if (!source) {
      container.textContent = m.name;
      return;
    }
    const previewSource = {
      ...source,
      html: m.preview || source.html,
      js: "",
    };
    renderWidget(container, previewSource, m.id, true);
    applyIcons(container);
    container.style.pointerEvents = "none";
  }

  search?.addEventListener("input", () => {
    filter = search.value.toLowerCase();
    render();
  });

  listen<Config>(EVENT.configUpdated, (e) => {
    cfg = e.payload;
    render();
  });

  render();
  logMemory("after mount");
  logInfo("widgets ready");
})();
