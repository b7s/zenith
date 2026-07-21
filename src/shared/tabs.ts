export interface TabDef {
  id: string;
  label: string;
}

export interface TabMount {
  container: HTMLElement;
  panes: Record<string, HTMLElement>;
  switchTo(id: string): void;
  readonly activeId: string;
}

export function mountTabs(parent: HTMLElement, tabs: TabDef[], initialId?: string): TabMount {
  const bar = document.createElement("nav");
  bar.className = "zen-tabs";
  bar.setAttribute("role", "tablist");

  const panes: Record<string, HTMLElement> = {};
  const buttons: Record<string, HTMLButtonElement> = {};
  let activeId = initialId ?? tabs[0]?.id ?? "";

  for (const def of tabs) {
    const btn = document.createElement("button");
    btn.className = "zen-tab";
    btn.setAttribute("role", "tab");
    btn.setAttribute("aria-selected", "false");
    btn.textContent = def.label;
    btn.dataset.tabId = def.id;

    const pane = document.createElement("div");
    pane.className = "zen-tab-pane";
    pane.setAttribute("role", "tabpanel");
    pane.hidden = true;

    bar.append(btn);
    parent.append(pane);
    buttons[def.id] = btn;
    panes[def.id] = pane;
  }

  function switchTo(id: string) {
    if (id === activeId || !buttons[id]) return;
    buttons[activeId].classList.remove("is-active");
    buttons[activeId].setAttribute("aria-selected", "false");
    panes[activeId].classList.remove("is-active");
    panes[activeId].hidden = true;
    activeId = id;
    buttons[activeId].classList.add("is-active");
    buttons[activeId].setAttribute("aria-selected", "true");
    panes[activeId].classList.add("is-active");
    panes[activeId].hidden = false;
  }

  bar.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-tab-id]");
    if (btn) switchTo(btn.dataset.tabId!);
  });

  if (activeId && buttons[activeId]) {
    buttons[activeId].classList.add("is-active");
    buttons[activeId].setAttribute("aria-selected", "true");
    panes[activeId].classList.add("is-active");
    panes[activeId].hidden = false;
  }

  return { container: bar, panes, switchTo, get activeId() { return activeId; } };
}
