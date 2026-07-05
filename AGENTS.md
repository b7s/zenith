# AGENTS.md — Zenith

> Operating contract for every contributor and AI agent working on **Zenith**.
> Follow these rules exactly. When in doubt, match the existing pattern, not your own preference.

---

## 1. What is Zenith?

Zenith is a **top bar for Windows 11** — a custom, always-available status bar that docks to
the top edge of the screen. It is inspired visually by **Cooldock** (macOS) and structurally by
**yasb** (Windows status bar).

Core ideas:

- **Stays on top and reserves space.** Zenith registers as a Windows **desktop AppBar**
  (`SHAppBarMessage`), so the shell shrinks the work area. Maximized windows stop *below* the bar
  and can never cover it — exactly like the native Taskbar.
- **Native transparency.** The bar, Settings, and Widget Manager windows use Windows **Acrylic**
  or **Mica** blur applied through the Win32 `SetWindowCompositionAttribute` accent API. The
  windows are fully transparent; the OS paints the blur. **CSS must never paint a background on
  these windows** — that would hide the native effect.
- **Widget system.** Widgets are small, standalone apps (plain JS/CSS/HTML) living in `widgets/`.
  Each has a `manifest.json`. Users toggle them on/off in the Widget Manager; their order and
  position (left/center/right) are saved to config.
- **Fully customizable visuals.** The Settings window (800×600) exposes the bar's material
  (Acrylic/Mica/None), tint transparency, background (transparent/solid/gradient), per-color
  transparency, corner rounding, edge margins, bar height, theme, and monitor selection. Changes
  apply live. Power users may additionally drop a `%APPDATA%\zenith\custom.css` that is hot-reloaded.
- **Right-click anywhere empty on the bar** → native context menu: **Settings · Widgets · Restart
  Bar · Close Bar**.
- **Custom chrome.** No window uses the Windows title bar. Every window has a custom header: semi-bold
  title on the left, `×` close on the right. The Widget Manager header also has a search input.
- **Minimal footprint.** Goal is the lowest possible RAM and CPU. No heavy framework, no
  per-window CSS backgrounds, compositor-friendly animations only.

---

## 2. Tech stack (use exactly these — latest stable)

| Layer | Technology |
|---|---|
| Shell / backend | **Rust** (edition 2021) |
| App framework | **Tauri 2** |
| Windows interop | `windows` crate **0.61** (`Win32_UI_Shell` for AppBar, `Win32_Graphics_Dwm` for corners, `Win32_Graphics_Gdi` for monitors, `SetWindowCompositionAttribute` for Mica/Acrylic) |
| Frontend | **plain TypeScript** (no React, no Vue) + **plain CSS** |
| Icons | **Lucide** |
| Design system | **shadcn design tokens** implemented in CSS (oklch on `:root`, `.dark`, `.light`) — *not* the React library |
| Build / bundler | **Vite 5** |
| Config format | JSON at `%APPDATA%\zenith\config.json` |

> **No React.** shadcn's *look* is reproduced with design tokens + reusable `.zen-*` CSS classes
> (see §6). This keeps bundle size and RAM minimal and matches the reference project (Plume).

---

## 3. Project structure (pragmatic hexagonal DDD)

Code is organized by **bounded context (domain)**, not by technical layer. Each domain owns its
model, pure service, and thin command adapter. Duplication is forbidden: a concern exists in
exactly one place.

```
zenith/
├── AGENTS.md                          # this file — the contract
├── README.md
├── package.json  tsconfig.json  vite.config.ts
├── index.html                         # bar window
├── settings.html                      # settings window (800×600)
├── widgets.html                       # widget manager window
├── src/
│   ├── shared/                        # SHARED KERNEL (frontend)
│   │   ├── ipc.ts                     # typed invoke() wrappers — single source of command names
│   │   ├── events.ts                  # event-name constants + typed listeners
│   │   ├── types.ts                   # DTO types mirroring Rust models
│   │   └── config.ts                  # config client (typed load + safe getter)
│   ├── domains/                       # frontend domain clients (config, appearance, widgets, …)
│   ├── windows/                       # thin window shells: bar/  settings/  manager/
│   └── styles/
│       ├── tokens.css                 # shadcn tokens (oklch): --bg --card --border --primary …
│       ├── base.css                   # reset, theme switch, scrollbars
│       ├── components.css             # .zen-* reusable component classes (see §6)
│       └── globals.css                # @import the three above; per-window CSS imports this
├── widgets/                           # standalone JS/CSS/HTML widgets
│   └── <name>/{manifest.json, widget.html, widget.js, widget.css}
└── src-tauri/
    ├── Cargo.toml  tauri.conf.json  build.rs
    ├── capabilities/{default,settings,widgets}.json   # per-window permissions
    └── src/
        ├── main.rs
        ├── lib.rs                     # composition root: plugins, commands, windows
        ├── shared/mod.rs              # SHARED KERNEL (backend): AppError, event consts, traits
        ├── config/                    # domain: configuration aggregate
        │   ├── model.rs               #   Config + sub-structs, #[serde(default)] everywhere
        │   ├── repository.rs          #   file load/save + safe-fallback getter
        │   └── commands.rs            #   thin #[tauri::command] adapters
        ├── window/                    # domain: window lifecycle
        │   ├── appbar.rs              #   SHAppBarMessage work-area reservation
        │   ├── transparency.rs        #   Mica/Acrylic — ONE implementation
        │   ├── monitor.rs             #   EnumDisplayMonitors
        │   └── commands.rs
        ├── appearance/                # domain: material/background/theme
        ├── widgets/                   # domain: widget registry (scan manifests) + positions
        ├── workspace/                 # domain: virtual-desktop COM (IVirtualDesktopManagerInternal)
        ├── motion/                    # domain: animation backend selection
        ├── gpu/                       # domain: GPU capability detect (cpu/vulkan/cuda/hip)
        ├── menu/                      # domain: native Win32 context menu
        └── dialog/                    # domain: OS dialogs (ChooseColor native color picker)
```

### DDD rules (read this before writing code)

1. **One concern, one place.** Transparency logic lives only in `window/transparency.rs`. AppBar
   only in `window/appbar.rs`. Never copy logic into a command or another domain.
2. **Pure services.** Domain services are plain Rust functions/structs with **no `tauri::` types**
   in their signatures. They take data in, return data out, and are unit-testable.
3. **Thin command adapters.** `#[tauri::command]` functions in `commands.rs` only: (a) receive
   `AppHandle`/args, (b) call a pure service, (c) emit an event if needed, (d) return the result.
   No business logic in commands.
4. **Cross-domain via shared kernel / events.** Domains do not reach into each other's internals.
   They emit/listen on event names defined once in `shared/` (`zenith:config-updated`,
   `zenith:appearance-changed`, etc.).
5. **Single source of truth.** Command names live only in `shared/ipc.ts`. Event names only in
   `shared/events.ts` (TS) and `shared/mod.rs` (Rust). DTO types defined once and mirrored.
6. **Config is the only mutable global state**, and it is always accessed through the `config`
   domain's `load()`/`save()` (see §5).

---

## 4. Configuration contract

- **Location:** `%APPDATA%\zenith\config.json` (i.e. `C:\Users\<user>\AppData\Roaming\zenith\`).
- **Format:** JSON. Unknown keys are tolerated (forward-compatible). Missing keys fall back to
  defaults. A corrupt file never crashes the app — it falls back to defaults.
- **No direct file reads outside the `config` domain.** All other domains call `config::load()`.

### Schema (top level)

```jsonc
{
  "appearance": {
    "material": "acrylic",              // "acrylic" | "mica" | "none"  (global toggle)
    "tint_alpha": 60,                   // 0..255 → accent gradient_color alpha (AABBGGRR)
    "background": {
      "mode": "transparent",            // "transparent" | "solid" | "gradient"
      "color_top": "#1a1a1a",
      "color_bottom": "#1a1a1a",
      "gradient_direction": "to_bottom",// "to_bottom" | "to_top"
      "alpha_top": 100,                 // 0..100
      "alpha_bottom": 100               // 0..100
    },
    "corner_radius": 8,                 // px
    "margin_top": 0, "margin_left": 0, "margin_right": 0,
    "bar_height": 40,                   // px
    "theme": "auto"                     // "auto" | "dark" | "light"
  },
  "monitors": "all",                    // "all" | ["<display_id>", ...]
  "layout": { "position": "top" },      // "top" (designed to extend to other edges later)
  "widgets": {
    "enabled": ["clock", "workspace", "volume", "battery"],   // order == left-to-right per zone
    "positions": { "clock": "left", "workspace": "left",
                   "volume": "right", "battery": "right" }    // "left" | "center" | "right"
  },
  "motion": { "backend": "auto", "reduced_motion": false },   // "auto" | "gpu" | "cpu"
  "css": { "custom_enabled": true }     // inject %APPDATA%\zenith\custom.css into the bar
}
```

---

## 5. How to read config — the safe getter (most important pattern)

**Goal:** any code can ask for config and *always* get a usable value — if the file is missing,
empty, or malformed, you get `Config::default()` (or a field default), never a panic or `unwrap`
failure.

This is achieved with three layers:

### Layer A — `#[serde(default)]` on EVERY field

Every struct field carries `#[serde(default)]` (or `#[serde(default = "fn")]` for non-empty
defaults). serde then fills any missing key with its default, so a partial/old config file always
deserializes.

### Layer B — a `Default` impl for every config struct

The aggregate (`Config`) and every sub-struct implement `Default`. This is the full fallback when
the file is absent or unparseable.

### Layer C — `load()` never errors

`config::load() -> Config` swallows all failures (missing file, IO error, invalid JSON) and returns
`Config::default()`, logging the reason. **Callers never handle `Result`.**

### Reference implementation (Rust) — `src-tauri/src/config/`

`model.rs` (excerpt):

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default = "default_monitors")]
    pub monitors: MonitorsSelection,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub widgets: WidgetsConfig,
    #[serde(default)]
    pub motion: MotionConfig,
    #[serde(default)]
    pub css: CssConfig,
}

fn default_monitors() -> MonitorsSelection { MonitorsSelection::All }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonitorsSelection { All, Only(Vec<String>) }
impl Default for MonitorsSelection { fn default() -> Self { MonitorsSelection::All } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_material")]   pub material: String,        // "acrylic"
    #[serde(default = "default_tint_alpha")] pub tint_alpha: u8,          // 60
    #[serde(default)]                        pub background: BackgroundConfig,
    #[serde(default = "default_corner_radius")] pub corner_radius: u32,   // 8
    #[serde(default)]                        pub margin_top: i32,
    #[serde(default)]                        pub margin_left: i32,
    #[serde(default)]                        pub margin_right: i32,
    #[serde(default = "default_bar_height")] pub bar_height: u32,         // 40
    #[serde(default = "default_theme")]      pub theme: String,           // "auto"
}
impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            material: "acrylic".into(), tint_alpha: 60, background: Default::default(),
            corner_radius: 8, margin_top: 0, margin_left: 0, margin_right: 0,
            bar_height: 40, theme: "auto".into(),
        }
    }
}
fn default_material() -> String { "acrylic".into() }
fn default_tint_alpha() -> u8 { 60 }
fn default_corner_radius() -> u32 { 8 }
fn default_bar_height() -> u32 { 40 }
fn default_theme() -> String { "auto".into() }

// … BackgroundConfig, LayoutConfig, WidgetsConfig, MotionConfig, CssConfig follow the same pattern.
```

`repository.rs` (the getter — this is the function you call):

```rust
use std::path::PathBuf;
use std::fs;
use crate::config::model::Config;

/// Resolve `%APPDATA%\zenith\config.json`. Falls back to a temp dir if APPDATA is unset.
pub fn config_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("zenith").join("config.json")
}

/// **THE getter.** Always returns a usable Config.
///
/// - File missing        → Config::default()
/// - File unreadable     → Config::default()  (logs the IO error)
/// - File invalid JSON   → Config::default()  (logs the parse error)
/// - File valid          → parsed Config (missing keys filled by serde defaults)
///
/// Never panics. Never returns Result. Call this everywhere.
pub fn load() -> Config {
    match try_load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("[zenith] config load failed ({e}); using defaults");
            Config::default()
        }
    }
}

fn try_load() -> Result<Config, String> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());                 // missing → defaults, not an error
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let cfg: Config = serde_json::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(cfg)
}

/// Read ONE field by JSON pointer path with a caller-supplied fallback.
/// Example: `config::get_or("/appearance/bar_height", 40)`
pub fn get_or<T>(pointer: &str, fallback: T) -> T
where
    T: for<'de> serde::Deserialize<'de>,
{
    let cfg = load();                                 // full safe load
    let raw = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
    match raw.pointer(pointer) {
        Some(v) => serde_json::from_value(v.clone()).unwrap_or(fallback),
        None => fallback,
    }
}

/// Persist config atomically (write to .tmp then rename).
pub fn save(cfg: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename → {}: {e}", path.display()))?;
    Ok(())
}
```

`commands.rs` (thin Tauri adapters — call the pure service, never hold logic):

```rust
use tauri::{AppHandle, Emitter};
use crate::config::{model::Config, repository};

#[tauri::command]
pub fn get_config() -> Config { repository::load() }   // never fails for the frontend

#[tauri::command]
pub fn save_config(app: AppHandle, config: Config) -> Result<bool, String> {
    repository::save(&config)?;
    app.emit(crate::shared::EVENT_CONFIG_UPDATED, &config).ok();
    Ok(true)
}
```

### How to USE it — from anywhere in Rust

```rust
// Whole config (preferred — cheap, safe, single source):
let cfg = zenith::config::load();
let material = cfg.appearance.material;        // always a real String, never None
let height = cfg.appearance.bar_height;        // always a real u32

// Or one value with explicit fallback:
let h = zenith::config::get_or("/appearance/bar_height", 40);
```

### How to USE it — from TypeScript

`src/shared/config.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./types";

/** Load the full config. Always resolves (backend never errors). */
export async function loadConfig(): Promise<Config> {
  return invoke<Config>("get_config");
}

/** Read a single nested value with a fallback (use sparingly — prefer loadConfig). */
export async function getConfigValue<T>(pointer: string, fallback: T): Promise<T> {
  // backend exposes get_or; see commands. For simple cases, loadConfig() + walk the object.
  const cfg = await loadConfig();
  const parts = pointer.split("/").filter(Boolean);
  let cur: unknown = cfg;
  for (const p of parts) {
    if (cur && typeof cur === "object" && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p];
    } else {
      return fallback;
    }
  }
  return (cur as T) ?? fallback;
}

export async function saveConfig(config: Config): Promise<void> {
  await invoke("save_config", { config });
}
```

> **Rule:** frontend code must go through `shared/config.ts`, never call `invoke("get_config")`
> directly. Command names are centralized in `shared/ipc.ts`.

---

## 6. CSS contract — the `.zen-*` component library (follow exactly)

All controls are built once in `src/styles/components.css` from the tokens in `tokens.css`.
**Never inline-style a control. Never invent a new field style.** Compose from these classes; if a
genuinely new primitive is needed, add it to `components.css` once.

Tokens (`tokens.css`, shadcn oklch on `:root`, overridden under `.dark`/`.light`):
`--bg --card --card-foreground --border --input --ring --primary --primary-foreground
--muted --muted-foreground --accent --radius --shadow`.

Reusable classes:

| Class | Purpose |
|---|---|
| `.zen-field` | wrapper: label + control + hint |
| `.zen-label` / `.zen-hint` | field text / muted helper |
| `.zen-input` / `.zen-textarea` | text fields |
| `.zen-select` + `.zen-select-wrapper` | styled native select |
| `.zen-slider` (+ `__track __range __thumb`) | range slider |
| `.zen-switch` | toggle switch |
| `.zen-checkbox` | checkbox |
| `.zen-radio-group` + `.zen-radio-card` | card-style radio group (shadcn radio-cards) |
| `.zen-tabs` + `.zen-tab` (`.is-active`) | tabs |
| `.zen-color-field` | swatch + hex (launches native Windows color picker) |
| `.zen-button` (`.is-primary .is-outline .is-ghost .is-destructive`, sizes `.is-sm .is-lg`) | buttons |
| `.zen-card` (+ `__header __title __content __footer`) | cards |
| `.zen-section` / `.zen-divider` | layout grouping |

Example (Settings field — correct):
```html
<div class="zen-field">
  <label class="zen-label" for="bar-height">Bar height</label>
  <input id="bar-height" class="zen-slider" type="range" min="28" max="72" />
  <p class="zen-hint">Pixels. Default 40.</p>
</div>
```
**Wrong** (do not do this): `<input style="appearance:none;background:#fff;border-radius:6px;...">`

---

## 6.1 Icon system (Lucide + Windows font fallback)

All icons flow through **one** module: `src/shared/icon.ts`.

- **Render:** either declarative `<i data-icon="battery" data-size="16"></i>` then `applyIcons(root)`,
  or programmatic `setIcon(el, "battery", { size: 16 })`.
- **Only icons in use are bundled.** `icon.ts` imports a small named set from `lucide` (app chrome +
  shipped widgets). This is tree-shaken — **never** `import { icons } from 'lucide'` (pulls all
  3000+ into memory). A new widget that needs an extra icon adds a named import to the registry, or
  calls `registerIcons({ name: node })` at runtime. (Per Lucide's own guidance:
  <https://lucide.dev/guide/installation> — "Recommended way, to include only the icons you need.")
- **Resolution order:** static registry → alias map → **Windows font fallback** using the glyph map
  in `src/shared/win-icons.ts`, rendered with the Segoe Fluent Icons → Segoe MDL2 Assets → Segoe UI
  Symbol font stack (in `src/styles/icons.css`). An unknown name always renders *something* (a
  placeholder square), never a blank.
- **Size** is honored for both SVG (`width`/`height`) and font glyphs (`font-size`). Default 16 px;
  pass `{ size }` or `data-size`.
- **Do not** deep-import `lucide/dist/esm/icons/<name>.js` at runtime. Those files are not shipped to
  `dist/`, so such imports 404 and silently fall back to the Windows glyph. Resolution must go
   through the registry / `registerIcons` so it stays tree-shaken and reliable.

### 6.2 Reusable TS components (`src/shared/`)

Shared UI components live as plain functions in `src/shared/*.ts`. Each accepts an `HTMLElement`
parent, builds DOM imperatively, and returns a controller handle. No JSX, no framework. Every
component must use `.zen-*` CSS classes exclusively.

| Module | Export | Purpose |
|---|---|---|
| `window.ts` | `mountWindow(opts)` | Builds header + content; returns `{ root, content, search }` |
| `tabs.ts` | `mountTabs(parent, TabDef[], initialId?)` | Builds tab bar + panes; returns `{ container, panes, switchTo, activeId }` |
| `icon.ts` | `setIcon(el, name, opts?)`, `applyIcons(root?)` | Renders Lucide SVG sprite or Win32 glyph |
| `config.ts` | `loadConfig()`, `saveConfig(cfg)`, `getConfigValue()` | Typed config client |
| `log.ts` | `initLog()`, `logInfo/Warn/Error()`, `logMemory()`, `time()` | Per-window file logging |
| `widgets.ts` | `loadWidgets()`, `renderWidget(manifest, zone)`, `layoutBar(config)` | Widget loading |

Pattern — tabs.ts (reference):
```ts
export interface TabDef { id: string; label: string; }
export interface TabMount {
  container: HTMLElement;
  panes: Record<string, HTMLElement>;
  switchTo(id: string): void;
  readonly activeId: string;
}
export function mountTabs(parent: HTMLElement, tabs: TabDef[], initialId?: string): TabMount {
  // Build nav.zen-tabs > button.zen-tab per def + div.zen-tab-pane per def
  // Wire click delegation on container
  // Return handle with switchTo/panes/activeId
}
```

Rules:
1. **One component, one file** — no bundling unrelated UI into a single module.
2. **Imperative DOM, no innerHTML** — use `document.createElement` / `append` to avoid XSS
   and keep the V8 JIT happy. Exception: widget content loaded from disk.
3. **Return a handle, not the DOM.** The caller should never need to query-select for
   the panes or buttons — they are returned as `Record<string, HTMLElement>`.
4. **Event delegation over binding.** Attach one listener to the component root
   instead of one per child element. This avoids memory churn from closure allocation.
5. **Use `.zen-*` classes exclusively.** Never inline styles or hardcode colors in TS.

## 7. Transparency contract (Windows Acrylic/Mica)

- Bar, Settings, and Widget Manager windows are `transparent: true, decorations: false`.
- Material is applied via `SetWindowCompositionAttribute` (`ACCENT_ENABLE_ACRYLICBLURBEHIND`,
  state 4) with a tint `gradient_color` of the form `AABBGGRR`. This is the **single**
  implementation, in `window/transparency.rs` (mirrors Plume's `apply_window_effects`).
- **No CSS `background` on these three windows' root elements.** The OS paints the blur; CSS only
  colors *content*. Optional solid/gradient background chosen in Settings is applied by toggling
  the Win32 accent to a solid tint or a thin content layer — never by CSS on the transparent root.
- The material toggle is **global** (Acrylic / Mica / None applies to all three windows). `"none"`
  → `ACCENT_DISABLED` (state 0).

---

## 8. Animation / motion contract (GPU + CPU fallback)

- "GPU" here means the **WebView2 compositor**. Animate **only** `transform`, `opacity`, and
  `filter` (these are composited on GPU for free). Add `will-change` only while animating, then
  remove it.
- **Avoid** animating `width/height/top/left/margin/box-shadow` (force layout / CPU).
- A `motion` domain reads `config.motion.backend` (`auto | gpu | cpu`) and `reduced_motion`. On
  `cpu` or `prefers-reduced-motion`, the app swaps to software easing / shorter transitions.
- `gpu/` detects capability (`cpu`, `vulkan` via `vulkan-1.dll`, `cuda` via `nvcuda.dll`, `hip`)
  only to *describe* the system and inform `auto` — **Zenith does not bundle CUDA/Vulkan binaries**
  (they would bloat RAM/CPU, contradicting the minimal-footprint goal).

---

## 9. Widget contract

- **Each widget is fully self-contained in its own folder.** All code lives in
  `widgets/<name>/` — the folder IS the widget. Adding a widget = create a folder with these
  files; removing a widget = delete the folder. No imports, no registration, no config outside
  the folder.

### 9.1 Folder layout

```
widgets/<name>/
├── manifest.json      # metadata (required)
├── widget.html        # HTML fragment (required) — injected into bar DOM
├── widget.js          # IIFE (optional) — runs once on mount
└── widget.css         # styles (optional) — injected once per session
```

- `manifest.json` fields: `name`, `id`, `version`, `description`, `default_zone` (`left|center|right`),
  `icon` (Lucide name), `min_width`, `preview` (thumbnail asset).
- `widget.html` is a plain HTML fragment (no `<html>`/`<body>` — just the content elements).
- `widget.js` wraps its logic in an IIFE to keep scope clean. It uses
  `window.__zenith_invoke` (set by the bar's `main.ts`) to call Tauri commands — never
  imports from `@tauri-apps/api` directly.
- `widget.css` is injected once per session into `<head>` (deduplicated by widget id).

### 9.2 Self-contained rule

There is no central widget registry to edit. The Rust `widgets` domain scans
`widgets/` at startup and on demand by reading each subdirectory's `manifest.json`.
The 3 source files (`widget.html`, `widget.js`, `widget.css`) are read into strings and
sent to the frontend via `get_widget_source` at layout time. Because every widget's code
lives in exactly one folder, adding/removing a widget is just `mkdir`/`rmdir`.

### 9.3 Widget lifecycle

- **Discovery:** `widgets::registry::scan_widgets()` reads `widgets/` subdirectories,
  parses `manifest.json`, and returns a `Vec<WidgetManifest>`. Invalid manifests are
  logged and skipped (never crash).
- **Source loading:** `widgets::registry::widget_source(id)` reads the 3 source files
  from `widgets/<id>/`. Missing files produce empty strings.
- **Layout:** `layoutBar()` in `src/shared/widgets.ts` iterates `cfg.widgets.enabled`,
  fetches each widget's source via IPC, injects CSS, sets `innerHTML` with the HTML
  fragment, and appends a `<script>` node for the JS IIFE.
- **Config** controls order (`enabled` array) and per-widget zone (`positions` map).
  To show/hide a widget, users toggle it in the Widget Manager (which edits
  `cfg.widgets.enabled`).

### 9.4 Sizing

- `min_width` in the manifest sets the widget slot's `min-width` in the bar. Use `0`
  for widgets that grow/shrink with content (e.g. workspace dots). Use a positive value
  for fixed-minimum widgets (e.g. clock at `80`).
- Widgets do not set their own width via CSS — the slot (`widget-slot`) controls it.

### 9.5 Widget right-click behavior

A widget may intercept right-click (`contextmenu` event) to show its own custom context menu.
When it does, it **must** call `e.preventDefault()` and `e.stopPropagation()` so the bar's
default context menu (Settings · Widgets · Restart Bar · Close Bar) does **not** appear when
right-clicking on that widget's area.

- A widget that does **not** handle `contextmenu` inherits the bar's default right-click menu.
- Widget context menus **must never be rendered as HTML divs** — the bar window is only 40px
  tall with `overflow: hidden`, so any HTML content outside the window bounds is clipped.
  Always use **Tauri's native `popup_menu` API** (`window.popup_menu(&menu)` in Rust, called
  via `invoke("show_<name>_context_menu")` from the frontend). Native menus are rendered by
  the OS outside the window bounds and match the system theme.
- The workspace widget builds its native menu dynamically in `commands.rs::build_workspace_menu()`:
  Rename, Delete (if >1 desktop), separator, Create New Desktop, separator, Move Window Here,
  Move Window To (submenu per desktop), separator, Toggle Pin Window. The menu ID prefix is
  `ws-`. Menu actions are handled in `handle_menu_event()` which emits typed events
  (`zenith:workspace-rename`, `zenith:workspace-delete`, etc.) back to the frontend.
- On the frontend side, the widget listens for these events via `__zenith_listen` and performs
  the corresponding `invoke()` calls (Rename opens a small Tauri input window, Delete uses a
  native MessageBoxW via `confirm_delete_desktop`).
- **Never use `prompt()` or `confirm()` in widget JS.** The bar window is only 40px tall and clips
  these dialogs. Use native Win32 dialogs via IPC instead (see §13.10).
- The right-clicked desktop ID is stored in `WS_CONTEXT_ID` (an `AtomicU32` in `commands.rs`)
  and passed as the event payload for rename/delete/move-here actions.
- Follow this pattern for any new widget that needs a custom right-click menu:
  1. Add a `show_<name>_context_menu` command in `commands.rs` that calls `build_<name>_menu()`.
  2. Add menu ID constants and extend `handle_menu_event()` to emit frontend events.
  3. In the widget's JS, prevent default contextmenu and `invoke("show_<name>_context_menu")`.
  4. Listen for the events and handle them with native dialogs via IPC (see §13.10).

### 9.6 Special rules

- **`workspace` widget:** shows one circle per virtual desktop; active desktop is a
  filled/colored circle, others outlined. Click to switch. **If there is only one
  virtual desktop, the bar layout layer (`layoutBar`) hides the widget automatically
  — even if the user enabled it.** Enumeration uses the undocumented
  `IVirtualDesktopManagerInternal` COM interface (the only way on Windows).

---

## 10. Window, base HTML & permissions contract

- **Shared base HTML.** All three windows (`index.html`, `settings.html`, `widgets.html`) use the
  identical skeleton — `<html data-theme="…">` → `<body data-window="bar|settings|widgets">` →
  `<div id="root">` → a single entry `main.ts`. **No window chrome is duplicated in HTML.** Each
  `main.ts` imports `src/styles/globals.css` once (the `@import` chain: tokens → base → components
  → icons → window).
- **`src/shared/window.ts` is the single source of the custom header.** `mountWindow({ title,
  searchable })` builds the semi-bold title (left) + optional search input + `×` close (right), wires
  close to `getCurrentWindow().close()`, and bootstraps theme + icons. It returns
  `{ root, content, search }` so the window fills `content` and (for the manager) wires `search`.
- **Drag region uses manual `pointerdown`, NOT `data-tauri-drag-region`.** The declarative
  attribute can swallow click events on interactive children (`button`, `input`) when the window
  is `transparent: true`. Instead, `mountWindow` calls `enableDrag(header)` which listens for
  `pointerdown` on the header, checks if the target is `button, input, select, textarea,
  [data-no-drag]`, and only starts dragging if it's not. This guarantees the close button and
  search field always receive click events. See `window.ts:enableDrag`.
- **Theme.** `applyTheme()` resolves `appearance.theme` (`auto|dark|light`) and sets `data-theme`;
  `watchSystemTheme()` keeps `auto` in sync with the OS. The bar window has no header (it *is* the
  bar), so it calls only `applyTheme()` + `applyIcons()` and builds its own strip.
- All windows: `decorations: false`, custom header via `mountWindow` (semi-bold title left, `×` right;
  manager adds a search input).
- One `capabilities/*.json` per window label (`default`=bar, `settings`, `widgets`). Never grant a
  permission globally that only one window needs. Close requires `core:window:allow-close`.
  Drag requires `core:window:allow-start-dragging`.
- Bar window: `alwaysOnTop: false` (the AppBar owns its band), `skipTaskbar: true`, `resizable: false`.
- Settings: 800×600, resizable. Widgets manager: resizable.
- Windows are created from `lib.rs` (composition root) or via
  `core:webview:allow-create-webview-window`.

---

## 11. Conventions

- **Rust:** edition 2021, `unwrap()`/`expect()` only in tests or after a proven invariant; prefer
  `?` with `String` error messages or `shared::AppError`. No `unsafe` outside `window/`,
  `workspace/`, and `gpu/`, and always with a safety comment.
- **TypeScript:** strict mode; no `any` in new code (use `unknown` + narrow). Import types with
  `import type`.
- **CSS:** tokens → components → window CSS. No inline styles. No `!important` unless overriding a
  third-party (rare).
- **Comments:** do **not** add comments unless asked. Code must be self-explanatory; the contract
  lives in this file.
- **Commits:** match the repo's existing style; never commit secrets or the `target/` and `dist/`
  build artifacts.
- **Testing:** pure domain services are unit-tested with `#[cfg(test)]` modules. The `config`
  domain must have tests proving: missing file → defaults, malformed JSON → defaults, partial file
  → defaults-with-overrides.

---

## 12. Reference: Plume patterns Zenith reuses

Zenith deliberately mirrors the **Plume** codebase (sibling project). When implementing a Windows
interop feature, check Plume first:

- Mica/Acrylic accent → Plume `lib.rs::apply_window_effects` (`SetWindowCompositionAttribute`).
- Rounded corners → Plume `lib.rs::force_window_corners_round` (`DwmSetWindowAttribute`).
- Dark/light detection → Plume `lib.rs::is_dark_mode` (registry `AppsUseLightTheme`) + poll loop.
- GPU detect + CPU fallback → Plume `gpu/mod.rs` (`LoadLibraryW`/`GetProcAddress`).
- Config with `#[serde(default)]` → Plume `config/mod.rs` (merge-on-save preserves unknown keys).

Re-implement in Zenith's domain structure — do not copy verbatim if the shape differs, but keep
the proven Win32 calls and fallback discipline.

---

## 13. Performance & memory safety rules

These rules are hard-won from bugs that caused system freezes, blank windows, and unclosable
dialogs. Follow them without exception.

### 13.1 Never block the Tauri main thread with a Win32 modal pump

`#[tauri::command]` handlers run on Tauri's main thread. Any call that enters a Win32 modal
message pump (e.g. `TrackPopupMenu`, `DialogBox`, `MessageBox`) will **block the IPC channel**.
Other windows' `invoke()` calls (including `get_config`, `save_config`) hang until the pump
returns, and the frontend appears blank and unresponsive.

- **Correct:** Use `window.popup_menu(&menu)` to show a context menu (returns immediately;
  Tauri dispatches the selected item via `on_menu_event`).
- **Wrong:** Hand-rolling `CreatePopupMenu` + `TrackPopupMenu` + `DestroyMenu` in a
  `#[tauri::command]`.

### 13.2 Manual drag region, not `data-tauri-drag-region`

The declarative `data-tauri-drag-region` can swallow `click` events on `button`/`input` children
when the window has `transparent: true`. This makes the close button and search field
intermittently unresponsive.

- **Correct:** A `pointerdown` listener on the header that calls
  `getCurrentWindow().startDragging()` only when the target is not an interactive child. See
  `src/shared/window.ts:enableDrag`.
- **Wrong:** `<header data-tauri-drag-region>` with no manual exclusion logic.

### 13.3 Timers and intervals: bound and cancel

Every `setInterval` / `setTimeout` in a widget or the bar creates a forever-running task.
Accumulating timers (e.g., re-layout loops) will exhaust the WebView's event loop.

- Widgets use exactly **one** `setInterval` if they need periodic updates (e.g. clock at 1 s).
- The bar's config watcher (Rust side) polls at most every **5 seconds**.
- Widgets that are removed from the DOM must also clear their intervals (store the timer id and
  call `clearInterval` on unmount). Later.

### 13.4 Icon loading: tree-shaken, never wildcard

- Import icons by name only: `import { X, Settings } from "lucide"`.
- **Never** `import { icons } from "lucide"` — this pulls all 3000+ icons into the bundle,
  bloating RAM and startup time.
- Register new icons via `registerIcons` or add a named import to the registry in
  `src/shared/icon.ts`. See §6.1.

### 13.5 Config is always safe, never unwrapped

- Every config access goes through `config::load()` or `config::get_or()`. These always return
  defaults — never panic, never block. See §5.
- Frontend config goes through `shared/config.ts`, never a raw `invoke("get_config")`.

### 13.6 CSS backgrounds on transparent windows

- Bar, Settings, and Widget Manager windows use Win32 Acrylic/Mica blur. **Never** paint a CSS
  `background` on the `<html>` or `<body>` of these windows — that would hide the native
  transparency effect and waste GPU compositing resources. See §7.

### 13.7 SVG icons: sprite, never duplicate path data

Each Lucide icon's SVG path data is stored **once** in a hidden `<svg>` sprite as a `<symbol>`,
then rendered via `<use href="#zen-i-<name>">`. This means N instances of the same icon share one
copy of the path data instead of cloning N SVG subtrees.

- The sprite lives at `document.documentElement` level, created lazily by `ensureSprite()` in
  `src/shared/icon.ts`.
- `setIcon` resolves the icon name, ensures a `<symbol>` exists, then inserts a lightweight
  `<svg><use href="#id"/></svg>`.
- **Never** call `createElement(node)` per icon instance outside of sprite setup.

### 13.8 Per-window CSS: load only what the window needs

The bar window imports `src/styles/bar-globals.css` (tokens + base + icons + bar) — **not**
`globals.css`, which also pulls in `components.css` (form controls) and `window.css` (window chrome).
Loading unused CSS wastes memory on parsed rule tables and style recalc.

- Bar: `bar-globals.css` (no `.zen-button`, `.zen-input`, `.zen-window`).
- Settings / Widgets: `globals.css` (full component library).

### 13.10 Native Win32 dialogs for widget interactions

The bar window is only 40px tall with `overflow: hidden`. JS built-in dialogs (`prompt`,
`confirm`, `alert`) are rendered by the WebView and are clipped by the window bounds —
the user sees nothing and the app appears frozen.

- **Never call `prompt()`, `confirm()`, or `alert()`** in widget JS or the bar itself.
- **Delete confirm:** Expose a `confirm_delete_desktop` async command that calls
  `MessageBoxW(MB_YESNO | MB_ICONQUESTION)` on a background thread via
  `tauri::async_runtime::spawn_blocking`. The IPC channel stays responsive because the
  blocking is off the main thread. Returns `bool`.
- **Rename input:** Do NOT use `prompt()`. Instead, open a small transient Tauri webview
  window (`rename.html` with a text field) via `show_rename_dialog`. The window is created
  by a `#[tauri::command]`, has its own title bar, and is unclipped. On submit it calls
  the rename IPC command and closes itself.
- **Pattern:** Menu handler emits a typed event → frontend calls the native IPC command →
  Rust shows the dialog on a worker thread → returns result → frontend refreshes.
- **Reuse the pattern** for any widget that needs user input or confirmation: create a thin
  Rust command that spawns the dialog off the main thread and returns the result.

### 13.11 Foreground HWND cache for move/pin operations

`GetForegroundWindow()` returns the bar's HWND when the user is interacting with the bar,
not the actual application window. This breaks "Move Window Here" and "Toggle Pin" because
the wrong window is moved/pinned.

Fix: Capture the foreground HWND **before** the native context menu takes focus, then store
it in a static `AtomicPtr<c_void>` in `workspace/commands.rs`. All move/pin IPC commands
read from this cache instead of calling `GetForegroundWindow()` at invocation time.

- `show_workspace_context_menu` → calls `set_foreground_hwnd(fg.0)` before `popup_menu`.
- `move_window_to_desktop`, `toggle_pin_window`, `pin_state` → call `get_cached_foreground_hwnd()`.
- `build_workspace_menu` → checks `get_cached_foreground_hwnd()` to decide whether to show
  move/pin menu items (instead of `GetForegroundWindow()`).

### 13.12 Restart must unregister the AppBar before spawning

Spawning a new process and exiting the old one leaves a brief overlap where both windows exist.
If the old AppBar is still registered, the new process can't claim the band and the user sees
two bars.

- **Correct:** `unregister_appbar` → `spawn` new exe → `app.exit(0)`.
  See `src-tauri/src/commands.rs:handle_menu_event`, `MI_RESTART`.

---

## 14. Logging & performance diagnostics

Zenith writes per-window logs to `%TEMP%/zenith/{YYYY-MM-DD}/{window}.log`, organized by date
so logs from different sessions don't overwrite each other. Use them to diagnose memory, startup
time, and IPC issues.

### 14.1 Log files

| File | Source |
|---|---|
| `%TEMP%/zenith/{date}/bar.log` | bar window (`index.html`) |
| `%TEMP%/zenith/{date}/settings.log` | settings window (`settings.html`) |
| `%TEMP%/zenith/{date}/widgets.log` | widget manager (`widgets.html`) |

Each entry: `[elapsed_seconds.mmm] [LEVEL] message`. Elapsed is relative to process start (not
wall clock). Open the files in any text editor, or tail them during dev.

### 14.2 Frontend API — `src/shared/log.ts`

```ts
import { initLog, logInfo, logWarn, logError, logMemory, time } from "../../shared/log";

await initLog();                    // truncate the window's log, write startup banner
logInfo("bar ready");               // append [INFO]
logWarn("config key missing");      // append [WARN]
logError("element not found");      // append [ERROR]
logMemory("startup");               // dump performance.memory (JS heap sizes)
await time("loadConfig", fn);       // measure async duration, log as "[INFO] loadConfig: 3.2ms"
```

Every window entry point calls `initLog()` + `logMemory("startup")` + `time()` around key
operations. This is **always on** (the overhead is negligible: one file append per call).

### 14.3 Rust API — `src-tauri/src/log.rs`

```rust
#[tauri::command]
pub fn log_write(window: String, level: String, message: String)  // append
#[tauri::command]
pub fn log_clear(window: String)                                   // truncate
```

Both are registered in `lib.rs::run()` via `invoke_handler`. They are zero-dependency (no
`chrono` — dates use a `SystemTime` algorithm, timestamps use `Instant::elapsed()`).

### 14.4 Performance budgets

| Operation | Budget | How to verify |
|---|---|---|
| Bar startup (init → ready) | < 200 ms | Check `bar.log` elapsed between `initLog` and `bar ready` |
| Window mount (mountWindow) | < 50 ms | `mountWindow: DOM build` line in log |
| Config load (IPC) | < 5 ms | `loadConfig: Xms` line in log |
| JS heap (bar, idle) | < 5 MB | `logMemory("startup")` / `logMemory("after layout")` |
| JS heap (settings, idle) | < 4 MB | `logMemory("after mount")` |

If any budget is exceeded, investigate before merging. The log files are the primary tool.

### 14.5 Reading logs after a freeze

1. Reproduce the freeze.
2. Force-kill the process if needed.
3. Open `%TEMP%/zenith/bar.log` (or the date-stamped copy for today's session).
4. Look for: last `logMemory` value (did heap grow?), large gaps between timestamps (did an
   operation block?), or repeated entries (a loop?).
5. Cross-reference with `settings.log` / `widgets.log` to see if IPC was stuck.
