import "../../styles/globals.css";
import { mountWindow, type MountedWindow } from "../../shared/window";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

export type ButtonVariant =
  | "primary"
  | "secondary"
  | "outline"
  | "ghost"
  | "destructive"
  | "danger"
  | "alert"
  | "info"
  | "success";

export type ButtonSize = "sm" | "md" | "lg";

export interface DialogAction {
  label: string;
  variant?: ButtonVariant;
  /** Size modifier. Default "md" (the base `.zen-button` height). */
  size?: ButtonSize;
  /** Remove background, border, and padding-chrome — renders as plain text link. */
  borderless?: boolean;
  /** Auto-focus this button when the dialog opens (only first one wins). */
  autofocus?: boolean;
  /** Called on click. Return false (or Promise<false>) to keep the dialog open. */
  onClick?(ctx: DialogContext): void | boolean | Promise<void | boolean>;
  /** Set to true to also fire on Enter key when this button is focused. */
  submitOnEnter?: boolean;
  /** If true (default), this action is also fired on Enter when an input
   *  inside the body has focus. Set false to prevent form-style submission. */
  submitOnNonButton?: boolean;
}

export interface DialogContext {
  close(): Promise<void>;
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  data: unknown;
  root: HTMLElement;
  content: HTMLElement;
}

export interface DialogOptions {
  title: string;
  /** Pre-fetched dialog data, available to the body builder via `ctx.data`. */
  data?: unknown;
  /** Build the scrollable body. If omitted, no <main> content is rendered. */
  body?: HTMLElement | ((ctx: DialogContext) => HTMLElement);
  /** Action buttons rendered in a fixed footer. Empty/omitted → no footer. */
  actions?: DialogAction[];
  /** Disable native browser context menu (default true in production). */
  disableContextMenu?: boolean;
  /** Disable text selection (default true). */
  disableSelect?: boolean;
  /** Close the dialog on Escape (default true). */
  closeOnEscape?: boolean;
  /** Called on every keydown inside the dialog. Return false to prevent default handling. */
  onKeyDown?: (e: KeyboardEvent, ctx: DialogContext) => void | false;
}

const VARIANT_CLASS: Record<ButtonVariant, string> = {
  primary: "is-primary",
  secondary: "is-secondary",
  outline: "is-outline",
  ghost: "is-ghost",
  destructive: "is-destructive",
  danger: "is-destructive",
  alert: "is-destructive",
  info: "is-outline",
  success: "is-primary",
};

export async function mountDialog(opts: DialogOptions): Promise<void> {
  const disableCtx = opts.disableContextMenu ?? !import.meta.env.DEV;
  const disableSelect = opts.disableSelect ?? true;
  const closeOnEscape = opts.closeOnEscape ?? true;

  const data = opts.data ?? null;

  const buttons = (opts.actions ?? []).map(a => {
    const b = document.createElement("button");
    const variant = a.variant ?? "outline";
    const size = a.size ?? "md";
    const classes = ["zen-button"];
    if (size === "sm" || size === "lg") classes.push(`is-${size}`);
    if (a.borderless) {
      classes.push("is-ghost");
      b.style.cssText = "background:transparent;border:none;padding:0 0.5rem;font-weight:500;";
    } else {
      classes.push(VARIANT_CLASS[variant] ?? "is-outline");
    }
    b.className = classes.join(" ");
    b.textContent = a.label;
    if (a.autofocus) b.autofocus = true;
    return b;
  });

  const { content, root }: MountedWindow = await mountWindow({
    title: opts.title,
    footer: buttons.length ? buttons : undefined,
  });

  const ctx: DialogContext = {
    close: () => getCurrentWindow().close(),
    invoke: (cmd, args) => invoke(cmd, args),
    data,
    root,
    content,
  };

  if (disableCtx) document.addEventListener("contextmenu", e => e.preventDefault());
  if (disableSelect) document.addEventListener("selectstart", e => e.preventDefault());

  if (opts.body) {
    const bodyEl = typeof opts.body === "function" ? opts.body(ctx) : opts.body;
    const wrapper = document.createElement("div");
    wrapper.style.cssText = "padding:1.25rem 1.25rem 0.5rem;display:flex;flex-direction:column;gap:1rem;min-height:0;flex:1 1 auto;overflow:auto";
    wrapper.append(bodyEl);
    content.append(wrapper);
  }

  (opts.actions ?? []).forEach((a, i) => {
    buttons[i].addEventListener("click", () => {
      const r = a.onClick?.(ctx);
      if (r && typeof (r as Promise<boolean>).then === "function") {
        (r as Promise<void | boolean>).then(shouldClose => { if (shouldClose !== false) void ctx.close(); });
      } else if (r !== false) {
        void ctx.close();
      }
    });
  });

  const autofocusBtn = (opts.actions ?? []).find(a => a.autofocus);
  if (autofocusBtn) {
    const idx = opts.actions!.indexOf(autofocusBtn);
    requestAnimationFrame(() => buttons[idx]?.focus());
  }

  document.addEventListener("keydown", (e) => {
    if (closeOnEscape && e.key === "Escape") { e.preventDefault(); void ctx.close(); return; }
    if (e.key === "Enter") {
      const target = e.target as HTMLElement;
      const idx = buttons.indexOf(target as HTMLButtonElement);
      if (idx >= 0 && opts.actions?.[idx]?.submitOnEnter !== false) {
        e.preventDefault();
        buttons[idx].click();
        return;
      }
      // Submit-on-Enter from non-button focus (e.g. text input): fire the
      // first action marked `submitOnNonButton: true` (defaults to the
      // primary/destructive action — same as a browser form submit).
      const candidateIdx = (opts.actions ?? []).findIndex(a => a.submitOnNonButton !== false);
      if (candidateIdx >= 0) {
        e.preventDefault();
        buttons[candidateIdx].click();
        return;
      }
    }
    const r = opts.onKeyDown?.(e, ctx);
    if (r === false) e.preventDefault();
  });

  let sizing = false;
  let lastW = 0, lastH = 0;
  function fitWindow() {
    void doFit();
  }
  async function doFit() {
    if (sizing) return;
    const maxW = 600, maxH = 600;
    const defaultW = 320, defaultH = 200;
    const headerH = 44;
    const footerEl = document.querySelector(".zen-window__footer") as HTMLElement | null;
    const footerH = footerEl?.offsetHeight ?? 0;
    const needW = content.scrollWidth + 4;
    const needH = content.scrollHeight + headerH + footerH + 6;
    const w = Math.min(Math.max(Math.max(defaultW, needW), 280), maxW);
    const h = Math.min(Math.max(defaultH, needH), maxH);
    if (Math.abs(w - lastW) <= 1 && Math.abs(h - lastH) <= 1) return;
    lastW = w; lastH = h;
    sizing = true;
    try {
      await getCurrentWindow().setSize(new LogicalSize(w, h));
      // Allow the resize event triggered by setSize to fire and be ignored before clearing
      await new Promise(requestAnimationFrame);
    } catch {}
    finally { sizing = false; }
  }
  requestAnimationFrame(() => requestAnimationFrame(fitWindow));
  window.addEventListener("resize", fitWindow);
}
