import "../../styles/globals.css";
import "./widget-config.css";
import { invoke } from "@tauri-apps/api/core";
import { mountWindow } from "../../shared/window";
import { mountTabs } from "../../shared/tabs";
import { mountFilterPills } from "../../shared/filter-pills";
import { initLog, logInfo } from "../../shared/log";
import { setIcon, applyIcons } from "../../shared/icon";
import { CMD } from "../../shared/ipc";
import type { Config, WidgetManifest, WidgetConfigField, CalendarAccount, PendingAuthStatus } from "../../shared/types";

interface WidgetConfigGlobals {
  __ZENITH_WIDGET_CONFIG_ID: string;
}

void (async () => {
  await initLog();

  const widgetId =
    (window as unknown as Partial<WidgetConfigGlobals>).__ZENITH_WIDGET_CONFIG_ID ?? "";

  if (!widgetId) {
    logInfo("no widget id provided");
    return;
  }

  const cfg = await invoke<Config>("get_config");
  const manifests = await invoke<WidgetManifest[]>("get_widgets");
  const manifest = manifests.find((m) => m.id === widgetId);

  if (!manifest) {
    logInfo(`manifest not found for widget id: ${widgetId}`);
    const { content } = await mountWindow({ title: "Widget Settings" });
    const hint = document.createElement("p");
    hint.className = "zen-hint";
    hint.style.padding = "1rem";
    hint.textContent = "Widget not found.";
    content.append(hint);
    return;
  }

  const configKeys = manifest.config ? Object.keys(manifest.config) : [];
  logInfo(`widget=${widgetId} config keys=[${configKeys.join(", ")}]`);

  if (!manifest.config || configKeys.length === 0) {
    const { content } = await mountWindow({ title: "Widget Settings" });
    const hint = document.createElement("p");
    hint.className = "zen-hint";
    hint.style.padding = "1rem";
    content.append(hint);
    return;
  }

  const savedValues =
    (cfg.widgets.config?.[widgetId] as Record<string, unknown> | undefined) ?? {};

  const configDef = manifest.config ?? {};

  // Client-id inputs for the datetime Calendars tab. Declared early so the
  // Save handler (below) can read them once populated by buildCalendarAccountsSection.
  const oauthClientIds: { google: HTMLInputElement | null; outlook: HTMLInputElement | null } = {
    google: null,
    outlook: null,
  };

  // ---- footer (built early so the window can mount with it) --------------
  const footerLeft = document.createElement("div");
  footerLeft.style.cssText = "display:flex;gap:0.5rem;align-items:center;";

  const footerRight = document.createElement("div");
  footerRight.style.cssText = "display:flex;gap:0.5rem;margin-left:auto;";

  const cancelBtn = document.createElement("button");
  cancelBtn.type = "button";
  cancelBtn.className = "zen-button is-outline";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  });
  footerRight.append(cancelBtn);

  const saveBtn = document.createElement("button");
  saveBtn.type = "button";
  saveBtn.className = "zen-button is-primary";
  saveBtn.textContent = "Save";
  saveBtn.addEventListener("click", async () => {
    const newValues: Record<string, unknown> = {};
    for (const [key, field] of Object.entries(configDef)) {
      if (field.type === "accounts") {
        newValues[key] = await collectAndProtectAccounts(key, accountStores);
      } else if (field.type === "bool") {
        newValues[key] = switchStates[key] ?? false;
      } else if (field.type === "int") {
        const el = inputs[key];
        const val = el instanceof HTMLInputElement ? el.value : "0";
        newValues[key] = parseInt(val, 10) || 0;
      } else if (field.type === "select") {
        const el = inputs[key];
        if (field.options && field.options.length <= 3) {
          const checked = el?.querySelector?.("input:checked") as HTMLInputElement | null;
          newValues[key] = checked?.value ?? field.value;
        } else {
          newValues[key] = (el as HTMLSelectElement | null)?.value ?? field.value;
        }
      } else if (field.type === "multiselect") {
        newValues[key] = multiStates[key] ?? [];
      } else {
        newValues[key] = (inputs[key] as HTMLInputElement | null)?.value ?? field.value;
      }
    }
    if (!cfg.widgets.config) cfg.widgets.config = {};
    // Preserve OAuth-connected calendars (managed by dedicated commands,
    // not the generic form) so Save doesn't wipe them.
    if (widgetId === "datetime" && savedValues.calendar_accounts) {
      newValues.calendar_accounts = savedValues.calendar_accounts;
    }
    if (widgetId === "datetime") {
      if (!cfg.calendar_oauth) cfg.calendar_oauth = { google_client_id: "", outlook_client_id: "" };
      cfg.calendar_oauth.google_client_id = oauthClientIds.google?.value.trim() ?? "";
      cfg.calendar_oauth.outlook_client_id = oauthClientIds.outlook?.value.trim() ?? "";
    }
    cfg.widgets.config[widgetId] = newValues;
    if (widgetId === "system_stats") {
      (cfg.widgets.config[widgetId] as Record<string, unknown>).selected_gpus = selectedGpus;
      (cfg.widgets.config[widgetId] as Record<string, unknown>).selected_hds = selectedHds;
      (cfg.widgets.config[widgetId] as Record<string, unknown>).selected_networks = selectedNetworks;
    }
    await invoke("save_config", { config: cfg });
    try { await invoke(CMD.gitRefresh); } catch { /* poll thread may not be running */ }
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  });
  footerRight.append(saveBtn);

  const { content, footer } = await mountWindow({ title: "Widget Settings", footer: [footerLeft, footerRight] });
  if (footer) footer.style.cssText = "display:flex;gap:0.5rem;padding:0.75rem 0.875rem 1rem;";

  const isGit = widgetId === "git";
  const isDatetime = widgetId === "datetime";
  const useTabs = isGit || isDatetime;

  // For the git and datetime widgets, split config into General + a second
  // tab (Credentials for git, Calendars for datetime).
  let generalPane: HTMLElement = content;
  let secondPane: HTMLElement = content;
  if (useTabs) {
    footerLeft.style.display = "none";
    const secondId = isGit ? "credentials" : "calendars";
    const secondLabel = isGit ? "Credentials" : "Calendars";
    const tabs = mountTabs(content, [
      { id: "general", label: "General" },
      { id: secondId, label: secondLabel },
    ]);
    content.prepend(tabs.container);
    generalPane = tabs.panes.general;
    secondPane = tabs.panes[secondId];
    tabs.container.addEventListener("click", (e) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-tab-id]");
      if (!btn) return;
      footerLeft.style.display = btn.dataset.tabId === secondId ? "flex" : "none";
    });
  }

  const form = document.createElement("div");
  form.className = "zen-section";

  function getPane(key: string): HTMLElement {
    if (isGit) return key === "accounts" ? secondPane : generalPane;
    if (isDatetime) return generalPane;
    return form;
  }

  const inputs: Record<string, HTMLElement> = {};
  const switchStates: Record<string, boolean> = {};
  const multiStates: Record<string, string[]> = {};

  // Dynamic hardware selection state (for system_stats)
  let selectedGpus: string[] = [];
  let selectedHds: string[] = [];
  let selectedNetworks: string[] = [];
  // Dynamic accounts state (for git widget)
  const accountStores: Record<string, AcctRow[]> = {};

  for (const [key, field] of Object.entries(configDef)) {
    const wrapper = document.createElement("div");
    wrapper.className = "zen-field";

    const currentValue = key in savedValues ? savedValues[key] : field.value;

    if (field.type === "accounts") {
      buildAccountsControl(wrapper, key, field, currentValue as Array<Record<string, unknown>> | undefined, accountStores, isGit ? footerLeft : undefined);
    } else if (field.type === "bool") {
      buildBoolControl(wrapper, key, field, currentValue, switchStates);
    } else if (field.type === "multiselect") {
      buildMultiSelectControl(wrapper, key, field, currentValue as string[] | undefined, multiStates);
    } else {
      const label = document.createElement("label");
      label.className = "zen-label";
      label.textContent = field.label || key;
      wrapper.append(label);
      buildControl(wrapper, key, field, currentValue, inputs);
      if (field.hint) {
        const hint = document.createElement("p");
        hint.className = "zen-hint";
        hint.textContent = field.hint;
        wrapper.append(hint);
      }
    }

    getPane(key).append(wrapper);
  }

  // Datetime widget: connected Google / Outlook calendars (OAuth). These
  // are managed by dedicated commands, not the generic form, so they live
  // outside `configDef` and must be preserved across Save (see the save
  // handler below).
  if (widgetId === "datetime") {
    const section = document.createElement("div");
    section.className = "zen-field";
    const calLabel = document.createElement("label");
    calLabel.className = "zen-label";
    calLabel.textContent = "Connected calendars";
    const calHint = document.createElement("p");
    calHint.className = "zen-hint";
    calHint.textContent =
      "Connect Google Calendar or Outlook to show events on the bar and alarm you at event start.";
    section.append(calLabel, calHint);
    secondPane.append(section);
    void buildCalendarAccountsSection(
      section,
      cfg.calendar_oauth ?? { google_client_id: "", outlook_client_id: "" },
      oauthClientIds,
    );
  }

  // Git: Credentials tab header with provider filter pills (after the title).
  if (isGit) {
    const credHeader = document.createElement("div");
    credHeader.className = "wc-cred-header";
    const credTitle = document.createElement("h3");
    credTitle.className = "wc-cred-title";
    credTitle.textContent = "Accounts";
    const pillsWrap = document.createElement("div");
    pillsWrap.className = "wc-cred-pills";
    credHeader.append(credTitle, pillsWrap);
    secondPane.prepend(credHeader);

    let currentFilter = "all";
    const providers = ["github", "gitlab", "forgejo", "gitea", "bitbucket"];
    const filter = mountFilterPills<string>(
      pillsWrap,
      [
        { id: "all", label: "All" },
        ...providers.map((p) => ({ id: p, label: p.charAt(0).toUpperCase() + p.slice(1) })),
      ],
      "all",
    );
    filter.container.addEventListener("click", (e) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-pill-id]");
      if (!btn) return;
      const next = btn.dataset.pillId as string;
      if (next === currentFilter) return;
      currentFilter = next;
      filter.container
        .querySelectorAll<HTMLButtonElement>("[data-pill-id]")
        .forEach((b) => b.classList.toggle("is-active", b.dataset.pillId === next));
      secondPane.querySelectorAll<HTMLElement>("[data-accts-key] .zen-field").forEach((row) => {
        const sel = row.querySelector("select");
        const pv = sel ? sel.value : "";
        row.style.display = currentFilter === "all" || pv === currentFilter ? "" : "none";
      });
    });
  }

  // Widgets without tabs render all fields in a single scrolling section.
  if (!useTabs) {
    const wrapper = document.createElement("div");
    wrapper.style.cssText = "flex:1;overflow-y:auto;overflow-x:hidden;padding:1rem;";
    content.append(wrapper);
    wrapper.append(form);
  }

  // For system_stats widget: show per-GPU and per-drive checkboxes
  if (widgetId === "system_stats") {
    try {
      const stats = await invoke<{
        gpu: { name: string; percent: number }[];
        hd: { mount: string; used: number; total: number; percent: number }[];
        network: { name: string; recv_bps: number; send_bps: number }[];
      }>("get_system_stats");

      const prevSelected = (savedValues.selected_gpus as string[] | undefined);
      const prevSelectedHd = (savedValues.selected_hds as string[] | undefined);
      const prevSelectedNet = (savedValues.selected_networks as string[] | undefined);
      const isFirstTime = !prevSelected && !prevSelectedHd && !prevSelectedNet;

      if (stats.gpu.length > 0) {
        const gpuSection = document.createElement("div");
        gpuSection.className = "zen-field";
        gpuSection.style.marginTop = "0.5rem";

        const gpuLabel = document.createElement("label");
        gpuLabel.className = "zen-label";
        gpuLabel.textContent = "GPUs to show";
        gpuSection.append(gpuLabel);

        for (let gi = 0; gi < stats.gpu.length; gi++) {
          const g = stats.gpu[gi];
          const checked = isFirstTime
            ? gi === 0
            : prevSelected
              ? prevSelected.includes(g.name)
              : true;
          if (checked) selectedGpus.push(g.name);
          buildHwCheckbox(gpuSection, g.name, checked, (isChecked) => {
            if (isChecked && !selectedGpus.includes(g.name)) selectedGpus.push(g.name);
            if (!isChecked) selectedGpus = selectedGpus.filter((x) => x !== g.name);
          });
        }

        form.append(gpuSection);
      }

      if (stats.hd.length > 0) {
        const hdSection = document.createElement("div");
        hdSection.className = "zen-field";
        hdSection.style.marginTop = "0.5rem";

        const hdLabel = document.createElement("label");
        hdLabel.className = "zen-label";
        hdLabel.textContent = "Drives to show";
        hdSection.append(hdLabel);

        const sysDrive = "C:";
        for (let hi = 0; hi < stats.hd.length; hi++) {
          const h = stats.hd[hi];
          const checked = isFirstTime
            ? h.mount === sysDrive
            : prevSelectedHd
              ? prevSelectedHd.includes(h.mount)
              : true;
          if (checked) selectedHds.push(h.mount);
          buildHwCheckbox(hdSection, h.mount, checked, (isChecked) => {
            if (isChecked && !selectedHds.includes(h.mount)) selectedHds.push(h.mount);
            if (!isChecked) selectedHds = selectedHds.filter((x) => x !== h.mount);
          });
        }

        form.append(hdSection);
      }

      if (stats.network && stats.network.length > 0) {
        const netSection = document.createElement("details");
        netSection.className = "zen-collapse";
        netSection.style.marginTop = "0.5rem";

        const netSummary = document.createElement("summary");
        netSummary.textContent = `Adapters to show (${stats.network.length})`;
        netSection.append(netSummary);

        const netBody = document.createElement("div");
        netBody.style.marginTop = "0.25rem";
        netBody.style.display = "grid";
        netBody.style.gap = "0.5rem";

        for (let ni = 0; ni < stats.network.length; ni++) {
          const n = stats.network[ni];
          const checked = isFirstTime
            ? ni === 0
            : prevSelectedNet
              ? prevSelectedNet.includes(n.name)
              : true;
          if (checked) selectedNetworks.push(n.name);
          buildHwCheckbox(netBody, n.name, checked, (isChecked) => {
            if (isChecked && !selectedNetworks.includes(n.name)) selectedNetworks.push(n.name);
            if (!isChecked) selectedNetworks = selectedNetworks.filter((x) => x !== n.name);
          });
        }

        netSection.append(netBody);
        form.append(netSection);
      }
    } catch {
      // ignore — hardware checkboxes won't show
    }
  }

})();

// Close on Escape.
document.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close().catch(() => {});
  }
});

/// Render the "Connected calendars" section for the datetime widget.
/// Accounts are managed entirely by the calendar-sync commands — this UI
/// only triggers connect / disconnect / sync and reflects state.
async function buildCalendarAccountsSection(
  parent: HTMLElement,
  oauthCfg: { google_client_id: string; outlook_client_id: string },
  clientIds: { google: HTMLInputElement | null; outlook: HTMLInputElement | null },
): Promise<void> {
  // User-supplied OAuth client ids. Empty → shipped placeholder ids.

  const gField = document.createElement("div");
  gField.className = "zen-field";
  const gLabel = document.createElement("label");
  gLabel.className = "zen-label";
  gLabel.textContent = "Google client id";
  const gInput = document.createElement("input");
  gInput.type = "text";
  gInput.className = "zen-input";
  gInput.placeholder = "e.g. 123456-abcdef.apps.googleusercontent.com";
  gInput.value = oauthCfg.google_client_id ?? "";
  gField.append(gLabel, gInput);
  clientIds.google = gInput;

  const oField = document.createElement("div");
  oField.className = "zen-field";
  const oLabel = document.createElement("label");
  oLabel.className = "zen-label";
  oLabel.textContent = "Outlook client id";
  const oInput = document.createElement("input");
  oInput.type = "text";
  oInput.className = "zen-input";
  oInput.placeholder = "e.g. 00000000-0000-0000-0000-000000000000";
  oInput.value = oauthCfg.outlook_client_id ?? "";
  oField.append(oLabel, oInput);
  clientIds.outlook = oInput;

  const oauthHint = document.createElement("p");
  oauthHint.className = "zen-hint";
  oauthHint.textContent =
    "Register Zenith as a desktop OAuth app to get a client id. Google: Google Cloud Console → APIs & Services → Credentials (Desktop app). Outlook: Microsoft Entra → App registrations (Mobile and desktop). Leave blank to use the built-in id.";

  parent.append(gField, oField, oauthHint);

  const actions = document.createElement("div");
  actions.style.cssText = "display:flex;gap:0.5rem;";
  const gBtn = document.createElement("button");
  gBtn.type = "button";
  gBtn.className = "zen-button is-outline";
  gBtn.textContent = "Connect Google";
  const oBtn = document.createElement("button");
  oBtn.type = "button";
  oBtn.className = "zen-button is-outline";
  oBtn.textContent = "Connect Outlook";
  actions.append(gBtn, oBtn);
  parent.append(actions);

  const status = document.createElement("p");
  status.className = "zen-hint";
  status.style.marginTop = "0.25rem";
  parent.append(status);

  const list = document.createElement("div");
  list.style.cssText = "display:flex;flex-direction:column;gap:0.5rem;margin-top:0.75rem;";
  parent.append(list);

  let pendingId: string | null = null;
  let pollTimer: number | null = null;

  const abortActive = () => {
    if (pendingId && pollTimer !== null) {
      const id = pendingId;
      if (pollTimer !== null) clearInterval(pollTimer);
      pollTimer = null;
      pendingId = null;
      void invoke(CMD.calendarAbortAuth, { pendingId: id });
    }
  };
  window.addEventListener("beforeunload", abortActive);

  async function render(): Promise<void> {
    const accounts = await invoke<CalendarAccount[]>(CMD.calendarAccountsList);
    list.replaceChildren();
    if (accounts.length === 0) {
      const empty = document.createElement("p");
      empty.className = "zen-hint";
      empty.textContent = "No calendars connected.";
      list.append(empty);
      return;
    }
    for (const acc of accounts) {
      const row = document.createElement("div");
      row.className = "zen-card";
      row.style.cssText = "display:flex;align-items:center;gap:0.5rem;padding:0.5rem 0.75rem;";
      const icon = document.createElement("i");
      icon.dataset.icon = acc.provider === "google" ? "calendar" : "mail";
      icon.dataset.size = "16";
      const name = document.createElement("div");
      name.style.cssText = "flex:1;min-width:0;";
      const title = document.createElement("div");
      title.textContent = acc.label || acc.account_email || acc.provider;
      const sub = document.createElement("div");
      sub.className = "zen-hint";
      sub.textContent =
        acc.provider +
        (acc.last_sync_at ? " · synced" : "") +
        (acc.last_error ? ` · error: ${acc.last_error}` : "");
      name.append(title, sub);

      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "zen-button is-sm" + (acc.enabled ? " is-primary" : " is-outline");
      toggle.textContent = acc.enabled ? "On" : "Off";
      toggle.addEventListener("click", async () => {
        await invoke(CMD.calendarSetEnabled, { accountId: acc.id, enabled: !acc.enabled });
        await render();
      });

      const syncBtn = document.createElement("button");
      syncBtn.type = "button";
      syncBtn.className = "zen-button is-sm is-ghost";
      syncBtn.textContent = "Sync";
      syncBtn.addEventListener("click", async () => {
        await invoke(CMD.calendarSyncNow);
        await render();
      });

      const discBtn = document.createElement("button");
      discBtn.type = "button";
      discBtn.className = "zen-button is-sm is-destructive";
      discBtn.textContent = "Disconnect";
      discBtn.addEventListener("click", async () => {
        await invoke(CMD.calendarDisconnect, { accountId: acc.id });
        await render();
      });

      row.append(icon, name, toggle, syncBtn, discBtn);
      list.append(row);
    }
    applyIcons(list);
  }

  async function beginConnect(provider: string, btn: HTMLButtonElement): Promise<void> {
    if (pendingId) return;
    try {
      status.textContent = `Opening ${provider} sign-in…`;
      const [pid, url] = await invoke<[string, string]>(CMD.calendarConnect, { provider });
      await invoke(CMD.openUrl, { url });
      pendingId = pid;
      btn.disabled = true;
      const start = Date.now();
      pollTimer = window.setInterval(async () => {
        const st = await invoke<PendingAuthStatus>(CMD.calendarPollAuth, { pendingId: pid });
        if (st.state === "pending") {
          if (Date.now() - start > 5 * 60 * 1000) {
            if (pollTimer !== null) clearInterval(pollTimer);
            pollTimer = null;
            pendingId = null;
            btn.disabled = false;
            status.textContent = "Timed out. Please try again.";
            await invoke(CMD.calendarAbortAuth, { pendingId: pid });
          }
          return;
        }
        if (pollTimer !== null) clearInterval(pollTimer);
        pollTimer = null;
        pendingId = null;
        btn.disabled = false;
        if (st.state === "ok") status.textContent = "Connected!";
        else if (st.state === "expired") status.textContent = "Session expired. Please try again.";
        else if (st.state === "error") status.textContent = `Connection failed: ${st.message}`;
        await render();
      }, 1500);
    } catch (e) {
      status.textContent = `Could not start connection: ${String(e)}`;
    }
  }

  gBtn.addEventListener("click", () => void beginConnect("google", gBtn));
  oBtn.addEventListener("click", () => void beginConnect("outlook", oBtn));

  await render();
}

function buildControl(
  wrapper: HTMLElement,
  key: string,
  field: WidgetConfigField,
  currentValue: unknown,
  inputs: Record<string, HTMLElement>,
): void {
  if (field.type === "select" && field.options) {
    if (field.options.length <= 3) {
      const group = document.createElement("div");
      group.className = "zen-radio-group";
      for (const opt of field.options) {
        const label = document.createElement("label");
        label.className = "zen-radio-card";
        if (String(opt) === String(currentValue)) label.classList.add("is-selected");
        const radio = document.createElement("input");
        radio.type = "radio";
        radio.name = `config-${key}`;
        radio.value = String(opt);
        if (String(opt) === String(currentValue)) radio.checked = true;
        radio.addEventListener("change", () => {
          group.querySelectorAll(".zen-radio-card").forEach((c) => c.classList.remove("is-selected"));
          label.classList.add("is-selected");
        });
        label.append(radio);
        const span = document.createElement("span");
        span.textContent = String(opt);
        label.append(span);
        group.append(label);
      }
      inputs[key] = group;
      wrapper.append(group);
    } else {
      const select = document.createElement("select");
      select.className = "zen-select";
      for (const opt of field.options) {
        const option = document.createElement("option");
        option.value = String(opt);
        option.textContent = String(opt);
        if (String(opt) === String(currentValue)) option.selected = true;
        select.append(option);
      }
      inputs[key] = select;
      const selectWrapper = document.createElement("div");
      selectWrapper.className = "zen-select-wrapper";
      selectWrapper.append(select);
      wrapper.append(selectWrapper);
    }
  } else if (field.type === "int") {
    const input = document.createElement("input");
    input.type = "number";
    input.className = "zen-input";
    input.value = String(currentValue ?? 0);
    inputs[key] = input;
    wrapper.append(input);
  } else {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "zen-input";
    input.value = String(currentValue ?? "");
    inputs[key] = input;
    wrapper.append(input);
  }
}

function buildBoolControl(
  wrapper: HTMLElement,
  key: string,
  field: WidgetConfigField,
  currentValue: unknown,
  switchStates: Record<string, boolean>,
): void {
  const checkbox = document.createElement("label");
  checkbox.className = "zen-checkbox";

  const text = document.createElement("span");
  text.className = "zen-checkbox__text";

  const label = document.createElement("span");
  label.className = "zen-checkbox__label";
  label.textContent = field.label || key;
  text.append(label);

  if (field.hint) {
    const desc = document.createElement("span");
    desc.className = "zen-checkbox__desc";
    desc.textContent = field.hint;
    text.append(desc);
  }

  checkbox.append(text);

  const switchEl = document.createElement("span");
  switchEl.className = "zen-checkbox__switch";

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = Boolean(currentValue);
  switchStates[key] = input.checked;
  input.addEventListener("change", () => {
    switchStates[key] = input.checked;
  });
  switchEl.append(input);

  const track = document.createElement("span");
  track.className = "zen-checkbox__track";
  const thumb = document.createElement("span");
  thumb.className = "zen-checkbox__thumb";
  track.append(thumb);
  switchEl.append(track);

  checkbox.append(switchEl);
  wrapper.append(checkbox);
}

function buildMultiSelectControl(
  wrapper: HTMLElement,
  key: string,
  field: WidgetConfigField,
  currentValue: string[] | undefined,
  states: Record<string, string[]>,
): void {
  const opts = field.options ?? [];
  const selected = new Set<string>(
    Array.isArray(currentValue) ? currentValue : ((field.value as string[]) ?? []),
  );
  states[key] = Array.from(selected);

  const fieldLabel = document.createElement("label");
  fieldLabel.className = "zen-label";
  fieldLabel.textContent = field.label || key;
  wrapper.append(fieldLabel);

  if (opts.length === 0) {
    const none = document.createElement("p");
    none.className = "zen-hint";
    none.textContent = "No options available.";
    wrapper.append(none);
    return;
  }

  const list = document.createElement("div");
  list.className = "zen-section";
  list.style.cssText = "display:flex;flex-direction:column;gap:0.35rem;margin-top:0.15rem;";
  for (const opt of opts) {
    const label = String(opt);
    const checkbox = document.createElement("label");
    checkbox.className = "zen-checkbox";

    const text = document.createElement("span");
    text.className = "zen-checkbox__text";
    const lbl = document.createElement("span");
    lbl.className = "zen-checkbox__label";
    lbl.textContent = label;
    text.append(lbl);
    checkbox.append(text);

    const switchEl = document.createElement("span");
    switchEl.className = "zen-checkbox__switch";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = selected.has(label);
    input.addEventListener("change", () => {
      if (input.checked) {
        if (!states[key].includes(label)) states[key].push(label);
      } else {
        states[key] = states[key].filter((x) => x !== label);
      }
    });
    switchEl.append(input);
    const track = document.createElement("span");
    track.className = "zen-checkbox__track";
    const thumb = document.createElement("span");
    thumb.className = "zen-checkbox__thumb";
    track.append(thumb);
    switchEl.append(track);

    checkbox.append(switchEl);
    list.append(checkbox);
  }
  wrapper.append(list);

  if (field.hint) {
    const hint = document.createElement("p");
    hint.className = "zen-hint";
    hint.textContent = field.hint;
    wrapper.append(hint);
  }
}

function buildHwCheckbox(
  wrapper: HTMLElement,
  name: string,
  checked: boolean,
  onChange: (isChecked: boolean) => void,
): void {
  const checkbox = document.createElement("label");
  checkbox.className = "zen-checkbox";

  const text = document.createElement("span");
  text.className = "zen-checkbox__text";

  const label = document.createElement("span");
  label.className = "zen-checkbox__label";
  label.textContent = name;
  text.append(label);

  checkbox.append(text);

  const switchEl = document.createElement("span");
  switchEl.className = "zen-checkbox__switch";

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  input.addEventListener("change", () => {
    onChange(input.checked);
  });
  switchEl.append(input);

  const track = document.createElement("span");
  track.className = "zen-checkbox__track";
  const thumb = document.createElement("span");
  thumb.className = "zen-checkbox__thumb";
  track.append(thumb);
  switchEl.append(track);

  checkbox.append(switchEl);
  wrapper.append(checkbox);
}

/* ── accounts type (git widget) ── */

interface AcctRow {
  key: string;
  id: string;
  label: HTMLInputElement;
  provider: HTMLSelectElement;
  username: HTMLInputElement;
  token: HTMLInputElement;
  hostUrl: HTMLInputElement;
  enabled: HTMLInputElement;
  el: HTMLElement;
  existingTokenBlob: string;
}

function buildAccountsControl(
  wrapper: HTMLElement,
  _key: string,
  _field: WidgetConfigField,
  currentValue: Array<Record<string, unknown>> | undefined,
  stores: Record<string, AcctRow[]>,
  addBtnTarget?: HTMLElement,
): void {
  const rows: AcctRow[] = [];
  stores[_key] = rows;

  const container = document.createElement("div");
  container.className = "zen-section";
  container.dataset.acctsKey = _key;

  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "zen-button is-outline is-sm";
  addBtn.textContent = "+ Add Account";
  addBtn.style.marginTop = addBtnTarget ? "0" : "0.25rem";

  function addRow(data?: Record<string, unknown>): void {
    const rowKey = crypto.randomUUID();
    const rowEl = document.createElement("div");
    rowEl.className = "zen-field";
    rowEl.style.cssText = "display:grid;grid-template-columns:1fr;gap:0.35rem;padding:0.5rem;border:1px solid color-mix(in oklch,var(--border) 50%,transparent);border-radius:var(--radius);";

    const labelInput = document.createElement("input");
    labelInput.type = "text";
    labelInput.className = "zen-input";
    labelInput.placeholder = "Label (e.g. Work)";
    labelInput.value = String(data?.label ?? "");

    const provider = document.createElement("select");
    provider.className = "zen-select";
    for (const pv of ["github", "gitlab", "forgejo", "gitea", "bitbucket"]) {
      const opt = document.createElement("option");
      opt.value = pv;
      opt.textContent = pv.charAt(0).toUpperCase() + pv.slice(1);
      if (pv === String(data?.provider ?? "github")) opt.selected = true;
      provider.append(opt);
    }

    const username = document.createElement("input");
    username.type = "text";
    username.className = "zen-input";
    username.placeholder = "Username";
    username.value = String(data?.username ?? "");

    const token = document.createElement("input");
    token.type = "password";
    token.className = "zen-input";
    const savedTokenBlob = data?.token_blob as string | undefined;
    const hasSavedToken = savedTokenBlob && savedTokenBlob.length > 0;
    token.placeholder = hasSavedToken ? "Token (leave blank to keep existing)" : "Token (required)";
    token.value = "";

    const tokenHint = document.createElement("p");
    tokenHint.className = "zen-hint";
    tokenHint.style.marginTop = "0.15rem";
    tokenHint.style.fontSize = "0.65rem";
    tokenHint.style.lineHeight = "1.3";

    function updateTokenHint(pv: string): void {
      const hints: Record<string, string> = {
        github: "Fine-grained personal access token (read-only). Create at GitHub Settings → Developer settings → Personal access tokens → Fine-grained tokens. Needs Actions, Contents & Pull requests: read access.",
        gitlab: "Personal access token with read_api scope. Create at GitLab Settings → Access Tokens.",
        forgejo: "Personal access token with repo read access. Create at Forgejo Settings → Applications → Manage Access Tokens. Also works for any Gitea instance.",
        gitea: "Personal access token with repo read access. Create at Gitea Settings → Applications → Manage Access Tokens.",
        bitbucket: "App password, API token, or repository access token. Uses HTTP Basic auth. If using an access token, any username works — we try x-token-auth automatically on 401.",
      };
      tokenHint.textContent = hints[pv] ?? "Personal access token with read-only access.";
    }
    updateTokenHint(provider.value);
    provider.addEventListener("change", () => updateTokenHint(provider.value));

    function updateHostHint(pv: string): void {
      hostHint.textContent = pv === "github" || pv === "gitlab"
        ? "Leave blank for cloud. For self-hosted, enter the base URL (e.g. https://gitlab.example.com)."
        : pv === "forgejo" || pv === "gitea"
          ? "Required. Enter the instance base URL (e.g. https://codeberg.org or https://git.example.com)."
          : "Leave blank for Bitbucket Cloud. For self-hosted Bitbucket Server, enter the base URL (e.g. https://bitbucket.example.com).";
    }

    const hostUrl = document.createElement("input");
    hostUrl.type = "text";
    hostUrl.className = "zen-input";
    hostUrl.placeholder = "Base URL (leave blank for cloud)";
    hostUrl.value = String(data?.host_url ?? "");

    const hostHint = document.createElement("p");
    hostHint.className = "zen-hint";
    hostHint.style.marginTop = "0.15rem";
    hostHint.style.fontSize = "0.65rem";
    hostHint.style.lineHeight = "1.3";
    updateHostHint(provider.value);
    provider.addEventListener("change", () => updateHostHint(provider.value));

    const enabledWrap = document.createElement("label");
    enabledWrap.className = "zen-checkbox";
    enabledWrap.style.flex = "1";
    const enabledText = document.createElement("span");
    enabledText.className = "zen-checkbox__text";
    const enabledLabel = document.createElement("span");
    enabledLabel.className = "zen-checkbox__label";
    enabledLabel.textContent = "Enabled";
    enabledText.append(enabledLabel);
    enabledWrap.append(enabledText);
    const enabledSwitch = document.createElement("span");
    enabledSwitch.className = "zen-checkbox__switch";
    const enabledInput = document.createElement("input");
    enabledInput.type = "checkbox";
    enabledInput.checked = data?.enabled !== false;
    enabledSwitch.append(enabledInput);
    const track = document.createElement("span");
    track.className = "zen-checkbox__track";
    const thumb = document.createElement("span");
    thumb.className = "zen-checkbox__thumb";
    track.append(thumb);
    enabledSwitch.append(track);
    enabledWrap.append(enabledSwitch);

    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className = "zen-icon-button";
    removeBtn.title = "Remove account";
    removeBtn.setAttribute("aria-label", "Remove account");
    setIcon(removeBtn, "trash-2", { size: 14 });
    removeBtn.addEventListener("click", () => {
      rowEl.remove();
      const idx = rows.findIndex((r) => r.key === rowKey);
      if (idx >= 0) rows.splice(idx, 1);
    });

    const topBar = document.createElement("div");
    topBar.style.cssText = "display:flex;align-items:center;gap:0.5rem;";
    topBar.append(enabledWrap, removeBtn);

    rowEl.append(topBar, labelInput, provider, hostUrl, hostHint, username, token, tokenHint);
    if (addBtnTarget) {
      container.append(rowEl);
    } else {
      container.insertBefore(rowEl, addBtn);
    }

    rows.push({ key: rowKey, id: String(data?.id ?? crypto.randomUUID()), label: labelInput, provider, username, token, hostUrl, enabled: enabledInput, el: rowEl, existingTokenBlob: String(data?.token_blob ?? "") });
    if (!data) labelInput.focus();
  }

  addBtn.addEventListener("click", () => addRow());
  if (addBtnTarget) {
    addBtnTarget.append(addBtn);
  } else {
    container.append(addBtn);
  }

  if (Array.isArray(currentValue)) {
    for (const acct of currentValue) addRow(acct);
  }

  wrapper.append(container);
}

async function collectAndProtectAccounts(key: string, stores: Record<string, AcctRow[]>): Promise<unknown[]> {
  const rows = stores[key] ?? [];
  const out: Record<string, unknown>[] = [];
  for (const row of rows) {
    const rawToken = row.token.value;
    let tokenBlob = "";
    if (rawToken.length > 0) {
      try {
        tokenBlob = await invoke<string>(CMD.protectSecret, { plaintext: rawToken });
      } catch {
        tokenBlob = "";
      }
    } else if (row.existingTokenBlob.length > 0) {
      tokenBlob = row.existingTokenBlob;
    }
    out.push({
      id: row.id,
      label: row.label.value,
      provider: row.provider.value,
      username: row.username.value,
      host_url: row.hostUrl.value,
      token_blob: tokenBlob,
      enabled: row.enabled.checked,
    });
  }
  return out;
}
