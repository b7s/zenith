# AGENTS.md — Zenith

> Operating contract for every contributor and AI agent working on **Zenith**.
> Follow these rules exactly. When in doubt, match the existing pattern, not your own preference.

---

## 1. What is Zenith?

Zenith is a **top bar for Windows 11** — a custom, always-available status bar that docks to
the top edge of the screen.

**Required Windows build: Windows 11 24H2 (build ≥ 26100.2605).** Zenith uses the
[`winvd`](https://docs.rs/winvd) crate to drive the virtual-desktop API, which only supports
24H2+. Users on older builds see a startup error and the app exits.

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

### Pending / known limitations

- **Workspace: "Move Window Here", "Move Window To", "Pin Window To/From All Desktops"** are
  currently disabled in the workspace context menu (replaced with a disabled "Move Window
  (Pending)" item). The Win32 `SetWinEventHook` for `EVENT_SYSTEM_FOREGROUND` returned
  `ERROR_HOOK_NEEDS_HMOD (1428)` from this Rust/Tauri binary (the OS expects a module-backed
  callback with `WINEVENT_OUTOFCONTEXT`), so the foreground-HWND cache never received any
  post-startup updates: move/pin always acted on whatever window was foreground when Zenith
  launched. Until a polling or HWND-injection solution is implemented (see
  `src-tauri/src/workspace/foreground.rs`), the workspace widget provides rename, delete,
  create, and switch only.

---

## 2. Tech stack (use exactly these — latest stable)

| Layer | Technology |
|---|---|
| Shell / backend | **Rust** (edition 2021) |
| App framework | **Tauri 2** |
| Windows interop | `windows` crate **0.61** (`Win32_UI_Shell` for AppBar, `Win32_Graphics_Dwm` for corners, `Win32_Graphics_Gdi` for monitors, `SetWindowCompositionAttribute` for Mica/Acrylic) + **`winvd` 0.0.49** for the virtual-desktop COM API (IVirtualDesktopManagerInternal / IVirtualDesktop). Requires a thin alias crate `windows_058 = { package = "windows", version = "0.58", features = ["Win32_Foundation"] }` to construct the HWND type `winvd` expects. |
| Frontend | **plain TypeScript** (no React, no Vue) + **plain CSS** |
| Icons | **Lucide** |
| Design system | **shadcn design tokens** implemented in CSS (oklch on `:root`, `.dark`, `.light`) — *not* the React library |
| Build / bundler | **Vite 8** |
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
├── src/
│   ├── windows/                         # thin window shells (HTML + TS) — see §10
│   │   ├── bar/index.html               # bar window entry
│   │   ├── settings/settings.html       # settings window (800×600)
│   │   ├── manager/widgets.html         # widget manager window
│   │   ├── dialog/dialog.html           # unified dialog window
│   │   ├── widget-config/widget-config.html   # generic widget-config window
│   │   └── calendar/calendar.html       # calendar popup
│   ├── shared/                          # SHARED KERNEL (frontend)
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
├── widgets/                           # standalone JS/CSS/HTML bar widgets (+ window shells for windows that pair with a widget)
│   └── <name>/{manifest.json, widget.html, widget.js, widget.css}   # bar widget (scanned by registry.rs)
│       └── window/<window>.html + main.ts + <window>.css            # OPTIONAL: Tauri window shell co-located with its widget (e.g. widgets/volume/window/volume-popup.html)
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
        ├── workspace/                 # domain: virtual-desktop wrapper (`winvd` crate) + foreground HWND tracking + event listener
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

### DRY — zero duplication, always (read this before writing ANY code)

The codebase is small and must stay small. Duplication is the #1 source of bugs here — a fix
made in one copy is forgotten in the other. **Before writing a function, a CSS rule, or a
command name, search the repo for an existing one.** The rules below are mandatory.

#### Before you write code — the 5-step duplication check

1. **Search first.** Use `Grep`/`Glob` for the concept you're about to implement — by name,
   by keyword, by type signature. The function you need probably already exists.
2. **Find the single home.** Every concern has exactly one file/module that owns it (see the
   "Where X lives" table below). New code for that concern goes **there**, not in a new file
   and not inlined into a caller.
3. **Extract before you copy.** If two callers need the same logic, **first** extract it into
   the owning module, **then** have both callers import it. Never paste-and-edit.
4. **Re-export, don't redeclare.** Names (commands, events, DTOs, CSS classes) are declared
   **once** in their owner and imported everywhere. If you need a value in two languages
   (Rust + TS), mirror it once and link the two with a comment — do not invent a parallel name.
5. **If you're about to write a 2nd copy of anything, stop.** Open the owning module and
   extend it instead. If no owner exists, create one and move both copies into it.

#### Where each concern lives (single home — do not create a second)

| Concern | Owner | Examples |
|---|---|---|
| Command names | `src/shared/ipc.ts` (`CMD`) | `CMD.getConfig`, `CMD.openWidgets` |
| Event names | `src/shared/events.ts` (`EVENT`) + `src-tauri/src/shared/mod.rs` | `zenith:config-updated`, `zenith:arrange-mode` |
| DTO types | `src/shared/types.ts` (+ Rust `model.rs` mirrored) | `Config`, `WidgetManifest` |
| Config load/save | `src/shared/config.ts` (TS) + `config/repository.rs` (Rust) | `loadConfig()`, `saveConfig()` |
| Window material (Mica/Acrylic) | `window/transparency.rs` | `apply_material`, `apply_fixed_acrylic` |
| AppBar | `window/appbar.rs` | `register_appbar`, `unregister_appbar` |
| Monitor lookup + popup clamping | `window/monitor.rs` | `clamp_to_monitor`, `clamp_rect_to_monitor` (§13.14) |
| Custom header + drag | `src/shared/window.ts` | `mountWindow()`, `enableDrag()` |
| Icon rendering | `src/shared/icon.ts` | `setIcon()`, `applyIcons()` |
| Per-window logging | `src/shared/log.ts` | `logInfo()`, `initLog()` |
| Widget loading/layout | `src/shared/widgets.ts` | `layoutBar()`, `getWidgets()` |
| Widget arrange / add / remove / move / drag-drop | `src/shared/widget-arrange.ts` | `addWidget()`, `applyArrangeUI()`, `setupBarDropZones()` |
| `.zen-*` CSS components | `src/styles/components.css` | `.zen-button`, `.zen-input`, `.zen-icon-button` (see §6.1a) |
| Arrange-mode CSS (sway, +/− buttons, drop-zones) | `src/styles/arrange.css` | `.zen-widget-btn`, `.is-drop-over` |
| Per-window CSS overrides | `src/styles/<window-name>.css` (e.g. `calendar.css`) | **size/color modifiers ONLY** — must compose with `.zen-*` (see §6.1a) |
| Rust `SetWindowPos` show pattern | `src-tauri/src/commands.rs::create_*_window` | the one `SWP_NOSIZE\|SWP_NOMOVE` call shape |

#### Anti-patterns that are forbidden

- **Inline-styling a control** (`style="..."`) when a `.zen-*` class exists for it. Add to
  `components.css` once instead.
- **Re-implementing the close button** with a `.clothes-shop-window__close` carrying its own
  `border`/`background`/`color`/`:hover` rules — inherit from `.zen-icon-button` +
  `.zen-window__close` and override only `width`/`height`. See §6.1a.
- **Re-implementing drag-and-drop / arrange logic** in a window's `main.ts`. It lives in
  `widget-arrange.ts`; import it.
- **Calling `invoke("get_config")` directly.** Go through `shared/config.ts`.
- **Hardcoding a command or event string** (`"zenith:config-updated"`) instead of importing
  `EVENT.configUpdated` / `CMD.*`.
- **Two `SetWindowPos` show sequences** with different flags. There is one correct shape
  (§13.10a/§13.10b); copy it verbatim.
- **A second CSS background on a transparent window.** There is one transparency contract (§7).
- **Mirroring a Rust struct into TS by hand** without keeping the two in sync. Edit both in the
  same change and cross-link them with a comment.

#### Glass / translucent control aesthetic — the only allowed fill pattern

Every control that sits on an Acrylic/Mica window (bar, settings, widgets manager, dialog)
must let the native blur show through. **Never use an opaque `background`** (`var(--success)`,
`var(--danger)`, a raw hex, or `oklch(... )` at full alpha) on a `.zen-*` class or any element
inside a transparent window. Always mix with transparency:

- **Fills:** `background: color-mix(in oklch, <token> 60–75%, transparent);` (range scales with
  how prominent the control is — buttons ~60–72%, cards ~75%, inputs ~55%).
- **Borders:** `border: 1px solid color-mix(in oklch, var(--border) 70%, transparent);` — never a
  hard 100% border.
- **Blur (optional, for floating chips):** `backdrop-filter: blur(8px) saturate(160%);` plus the
  `-webkit-` prefix. Only on small floating elements (e.g. `.zen-widget-btn`); the window already
  has the OS-wide blur.
- **Hover/active:** change the mix percentage (`60% → 78%`) or add an `opacity` transition — never
  swap to an opaque fill.

Reference implementations: `.zen-button` (`components.css`), `.zen-card`, `.zen-input`,
`.zen-widget-btn` (`arrange.css`). When creating a new control, copy the `color-mix` fill pattern
from the closest existing one and adjust the percentage only.

#### How AI agents specifically avoid duplication

- **Prefer one large edit to many small ones.** When a concern spans Rust + TS, do both edits
  in the same turn so they cannot drift.
- **Read the owner before extending it.** Don't guess the API — `Read` the owning module,
  match its existing style, then add.
- **When asked for a new feature, first ask "which existing module owns this?"** If the answer
  is "none", the first task is to create the owner, not to spread the feature across callers.
- **Treat copy-paste as a build failure.** If your diff contains two near-identical blocks,
  extract a helper. The reviewer (human or AI) will ask for it anyway.

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
    "tint_alpha": 102,                   // 0..255 → accent gradient_color alpha (AABBGGRR)
    "background": {
      "mode": "transparent",            // "transparent" | "solid" | "gradient"
      "color_top": "#1a1a1a",
      "color_bottom": "#1a1a1a",
      "gradient_direction": "to_bottom",// "to_bottom" | "to_top"
      "alpha_top": 100,                 // 0..100
      "alpha_bottom": 100               // 0..100
    },
    "corner_radius_tl": 0,             // px
    "corner_radius_tr": 0,
    "corner_radius_br": 0,
    "corner_radius_bl": 0,
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
    #[serde(default = "default_tint_alpha")] pub tint_alpha: u8,          // 102
    #[serde(default)]                        pub background: BackgroundConfig,
    #[serde(default = "default_corner_radius")] pub corner_radius_tl: u32, // 0
    #[serde(default = "default_corner_radius")] pub corner_radius_tr: u32, // 0
    #[serde(default = "default_corner_radius")] pub corner_radius_br: u32, // 0
    #[serde(default = "default_corner_radius")] pub corner_radius_bl: u32, // 0
    #[serde(default)]                        pub margin_top: i32,
    #[serde(default)]                        pub margin_left: i32,
    #[serde(default)]                        pub margin_right: i32,
    #[serde(default = "default_bar_height")] pub bar_height: u32,         // 40
    #[serde(default = "default_theme")]      pub theme: String,           // "auto"
}
impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            material: "acrylic".into(), tint_alpha: 102, background: Default::default(),
            corner_radius_tl: default_corner_radius(), corner_radius_tr: default_corner_radius(),
            corner_radius_br: default_corner_radius(), corner_radius_bl: default_corner_radius(),
            margin_top: 0, margin_left: 0, margin_right: 0,
            bar_height: 40, theme: "auto".into(),
        }
    }
}
fn default_material() -> String { "acrylic".into() }
fn default_tint_alpha() -> u8 { 102 }
fn default_corner_radius() -> u32 { 0 }
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
| `.zen-checkbox` (+ `__text __label __desc __switch __track __thumb`) | Apple-style toggle switch — label + optional description on the left, toggle on the right |
| `.zen-radio-group` + `.zen-radio-card` | card-style radio group (shadcn radio-cards) |
| `.zen-tabs` + `.zen-tab` (`.is-active`) | tabs |
| `.zen-color-field` | swatch + hex (launches native Windows color picker) |
| `.zen-button` (`.is-primary .is-outline .is-ghost .is-destructive`, sizes `.is-sm .is-lg`) | buttons |
| `.zen-icon-button` | round ring-style icon button (used for nav chevrons, window close ×, popup close) |
| `.zen-card` (+ `__header __title __content __footer`) | cards |
| `.zen-section` / `.zen-divider` | layout grouping |
| `.zen-window` (+ `__header __title-wrap __title __title-badge __search __search-icon __search-input __close __content __footer`) | frameless window chrome — header / search / content / footer layout, drag region via `enableDrag()` |

Example (Settings field — correct):
```html
<div class="zen-field">
  <label class="zen-label" for="bar-height">Bar height</label>
  <input id="bar-height" class="zen-slider" type="range" min="28" max="72" />
  <p class="zen-hint">Pixels. Default 40.</p>
</div>
```
**Wrong** (do not do this): `<input style="appearance:none;background:#fff;border-radius:6px;...">`

### 6.1a `.zen-*` lookup decision tree (use before writing ANY new CSS)

Before adding a new class to `components.css` or any window/popup CSS, **walk this tree in order**:

1. **Is there already a `.zen-*` class that does this?** Grep `src/styles/*.css` for the
   concept (not just the name). Examples of common matches people miss:
   - *Icon button*: `.zen-icon-button` — covers the ring-style background, hover, border, color.
     **Use it for every `×` close, every nav chevron, every icon-only button.** Per-window CSS
     then adds ONLY a size modifier (e.g. a popup's smaller `.cal-window__close { width: 1.5rem…
     }`).
   - *Window chrome*: `.zen-window` + `.zen-window__*` — header / close / content / footer.
     Popups compose these instead of building fresh `.cal-window__*` classes.
   - *Native select*: `.zen-select` + `.zen-select-wrapper`. Only deviate when the visual
     contract is intentionally different (e.g. the calendar's tinted year picker).
   - *Form layout*: `.zen-field` + `.zen-label` + `.zen-hint` are the canonical
     label / control / helper stack; never nest a custom `.field-*` div with its own padding.
2. **Is the existing class the wrong SIZE only?** Add a tiny size modifier class (e.g.
   `.cal-nav { width: 1.5rem; height: 1.5rem }`) that inherits everything else. Do **not**
   duplicate the background / border / color / hover rules.
3. **Is the existing class the wrong VISUAL CONCEPT?** (e.g. the year picker is a tinted
   primary button, not a `.zen-select`-style dropdown.) Use it, but document the deviation
   with a comment that names the shared class you considered.
4. **Nothing exists?** Add to `src/styles/components.css` as a `.zen-*` primitive and link
   from the table above. Don't ship a window-only class as a substitute.

**Forbidden duplication examples** (caught in review before, listed so reviewers can spot
them instantly):
- Defining `.cal-window__close` with `border`/`background`/`color`/`:hover` again —
  inherit from `.zen-icon-button` + `.zen-window__close`, override only `width`/`height`.
- Defining `.cal-window__content` with the full flex-stack — inherit from
  `.zen-window__content`, override only `padding`/`gap`.
- Defining `.cal-nav` from scratch — start from `.zen-icon-button`, override only the
  size.
- Building a custom popup `<header>` with `display:flex; justify-content:flex-end…` —
  `.zen-window__header` already provides the flex layout; popups just override
  `justify-content` / `padding` / `height`.

**The mirror test.** If your new class shares three or more properties with an existing
`.zen-*` class, you are duplicating — extract the common properties into the shared class
or use composition (`class="zen-icon-button cal-window__close"`).

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

#### Widget icons: avoid duplicating SVG paths

When a widget needs a custom SVG icon (not a Lucide icon):

1. **Define the SVG in `widget.js`** using `document.createElementNS`. Build the SVG node tree
   programmatically in the widget's IIFE and append it to the container. This keeps the SVG path
   data in **one place** — the JS file.
2. **Keep `widget.html` minimal** — just the structural wrapper elements. No inline SVG markup.
3. **`manifest.json` preview** is exempt (it's a static inert fragment with no JS, per §9.1).
   The preview string necessarily duplicates the SVG markup — this is the expected cost of a
   JS-free preview.

**Example** (from `widgets/git/widget.js`):
```ts
var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
svg.setAttribute("viewBox", "0 0 78 78");
svg.setAttribute("fill", "currentColor");
svg.classList.add("git-ic");
var path = document.createElementNS("http://www.w3.org/2000/svg", "path");
path.setAttribute("d", "…");
svg.append(path);
wrap.append(svg);
```

**Wrong**: inlining `<svg>…</svg>` in `widget.html` when the same path also appears in
`manifest.json`'s preview — now the path lives in two files and a fix to one is forgotten
in the other.

### 6.2 Reusable TS components (`src/shared/`)

Shared UI components live as plain functions in `src/shared/*.ts`. Each accepts an `HTMLElement`
parent, builds DOM imperatively, and returns a controller handle. No JSX, no framework. Every
component must use `.zen-*` CSS classes exclusively.

| Module | Export | Purpose |
|---|---|---|
| `window.ts` | `mountWindow(opts)` | Builds header + content; returns `{ root, content, search }` |
| `tabs.ts` | `mountTabs(parent, TabDef[], initialId?)` | Builds tab bar + panes; returns `{ container, panes, switchTo, activeId }` |
| `filter-pills.ts` | `mountFilterPills(parent, PillDef[], initialId?)` | Segmented pill toggle (All / X / Y); returns `{ container, switchTo, activeId }` |
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

#### Segmented filter pills (`src/shared/filter-pills.ts`)

Use `mountFilterPills` when a window needs a small horizontal segmented control
above a list/grid where **exactly one** option is active at a time — typically
2–6 mutually-exclusive "view modes" of the same dataset.

**Use this when:**

- Filtering a single list by a facet: `All / Event / Alarm`, `Active / Done`,
  `Today / Week / Month`, `Logs / Errors / Warns / Info`.
- Switching the visualization of one dataset without leaving the page
  (`List / Grid`, `Compact / Detailed`).
- You need the active state to live across re-renders (the mount is
  idempotent — mount once, then call `switchTo` on every render to sync the
  active class).

**Do NOT use this when:**

- You need **multi-select** (independent toggles stacked horizontally) — that's a
  checkbox group, not a filter pill.
- You need **navigation between distinct pages** (e.g. switching "Settings" → "About")
  — that's `mountTabs`, which pairs buttons with panels (`zen-tab-pane`).
- You need >6 options — a `zen-select` is more compact.
- The choice is **destructive or one-shot** (Save / Delete / Confirm) — use
  `.zen-button` variants instead.

**How it works:**

```ts
import { mountFilterPills, type FilterPillsMount } from "@/shared/filter-pills";

type Mode = "all" | "event" | "alarm";
let mode: Mode = "all";
const wrap = document.createElement("div");
const pills = mountFilterPills<Mode>(wrap, [
  { id: "all",   label: "All" },
  { id: "event", label: "Event" },
  { id: "alarm", label: "Alarm" },
], mode);

// Controlled state — `mountFilterPills` toggles `is-active` for you, but the
// caller decides what to do with the id. Listen via event delegation on the
// returned container (or override the click listener entirely).
pills.container.addEventListener("click", (e) => {
  const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-pill-id]");
  if (!btn) return;
  const next = btn.dataset.pillId as Mode;
  if (next === mode) return;
  mode = next;
  render(); // your list/grid rebuilds with the new filter
});

// To re-sync class state without changing the id (e.g. after config reload):
pills.switchTo(mode);
```

**Visual contract** (lives in `src/styles/components.css`):

- `.zen-filter-pills` — pill-rounded container, transparent fill so the window's
  Acrylic/Mica shows through (`color-mix(in oklch, var(--card) 35%, transparent)`).
- `.zen-filter-pill` — 1.5rem tall, `1.5rem`-rounded. Hover = soft accent bg;
  active = primary fill + primary-tinted border.
- All animations are `transition` on `background`, `color`, and `border-color`
  only — compositor-only properties per §8.
- The container takes its size from its parent (it's `display: inline-flex`).

**Mirrors the `mountTabs` API exactly** so swapping between the two is a one-line
change if you realize mid-build that you wanted the wrong one. The key visual
difference: tabs are anchored to a bottom border (page chrome), pills float
(pure filter).

**Overflow → "More" dropdown (automatic — no caller work):**

When the pills exceed the width of their container, `mountFilterPills` keeps the
**active** pill visible and collapses the rightmost non-active pills into a
"More ▾" dropdown anchored to the pill bar (`.zen-filter-pills__menu`). Selecting
an item — or resizing the window — re-distributes the pills so nothing is ever
cut off or lost. The dropdown items are real `.zen-filter-pill` elements with
`data-pill-id`, so the existing click delegation (and the consumer's own
`container` click listener) keep working unchanged. This is global: the git-manager
account filter and the git widget-config provider filter both stay usable with
many options because they share this one implementation.

For overflow to trigger, the pills' container must be **width-constrained** — it
must have a bounded width to measure against. The component measures
`parent.clientWidth` (the element passed to `mountFilterPills`). Give that parent
`flex: 1 1 auto; min-width: 0;` and let it live in a bounded flex row. **Never set
`overflow: hidden` on that parent** — it would clip the dropdown; the component
collapses pills *instead* of clipping, so the dropdown always has room to show.
Example (git widget-config Credentials header):

```css
.wc-cred-pills { margin-left: auto; flex: 1 1 auto; min-width: 0; }
```

The "More" button + menu styles live in `components.css`
(`.zen-filter-pill-more`, `.zen-filter-pills__menu`) — never re-implement them,
and never append your own overflow logic in a window's `main.ts`.

**Reference consumer:** `src/windows/calendar/main.ts::renderEventsView` — the
shared module is mounted once outside the render loop, `switchTo` is called on
every re-render to keep the active class in sync without rebuilding the DOM.

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
  `icon` (Lucide name), `min_width`, `preview` (static HTML fragment — fake sample content shown
  in the Widget Manager card only; never rendered in the bar). The preview reuses the widget's own
  `widget.css` classes so it looks identical to the live widget, but **no `widget.js` runs** and the
  container has `pointer-events: none`, so the preview is fully inert. Example: the clock manifest
  ships `"preview": "<span class=\"clock-time\">12:34</span>""`; the workspace manifest ships three
  `.ws-dot` spans (one `.is-active`). If `preview` is empty/absent, the manager falls back to the
  real `widget.html` (still without JS).
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

### 9.4a Widget configuration (`config` in manifest)

Widgets that need user-configurable settings declare them in `manifest.json` under the
top-level `config` key. When a widget has a non-empty `config`, the Widget Manager card
shows a **gear button** (below the add/remove button) that opens a **single generic
widget-config window** (`src/windows/widget-config/widget-config.html`). This window is data-driven — it reads the
manifest's `config` definition and renders the appropriate form controls via JS. **Never
create a per-widget config window** — always extend the generic one.

#### Manifest `config` schema

```jsonc
{
  "config": {
    "timezone": {              // key = the config field name
      "type": "string",        // "string" | "int" | "bool" | "select"
      "value": "",             // default value (used when user hasn't configured)
      "label": "Timezone",     // human-readable label (optional, falls back to key)
      "hint": "IANA tz …"      // helper text under the field (optional)
    },
    "format": {
      "type": "select",
      "value": "24h",
      "options": ["24h", "12h"],   // required for "select" type
      "label": "Time format"
    },
    "show_date": {
      "type": "bool",
      "value": true,
      "label": "Show date"
    }
  }
}
```

Each config field has **two required entries**: `type` and `value`. The type determines
which `.zen-*` control the generic window renders:

| `type` | Control | Notes |
|---|---|---|
| `"string"` | `.zen-input` (text) | Free-text input |
| `"int"` | `.zen-input` (number) | Integer input |
| `"bool"` | `.zen-checkbox` | Toggle |
| `"select"` | `.zen-select` (dropdown) | Requires `options` array of strings/numbers |

Optional entries: `label`, `hint`, `options` (for select).

#### How values are stored

Config values live in `config.json` under `widgets.config`:

```jsonc
{
  "widgets": {
    "enabled": ["datetime"],
    "positions": { "datetime": "center" },
    "config": {
      "datetime": { "timezone": "America/New_York", "format": "12h", "show_date": true }
    }
  }
}
```

If a widget's key is absent from `widgets.config[<id>]`, the widget JS uses the manifest's
default `value`.

#### How widget JS reads its config

The widget IIFE calls `window.__zenith_invoke("get_config")`, navigates to
`cfg.widgets.config[<widget-id>]`, and falls back to manifest defaults for any missing key.

#### How the config window works (single generic implementation)

- **Rust:** `open_widget_config(app, widget_id)` in `commands.rs` creates a
  `widget-config-<id>` window with an init script `window.__ZENITH_WIDGET_CONFIG_ID`.
- **Window JS:** `src/windows/widget-config/main.ts` reads the widget ID, fetches
  the manifest (`get_widgets`) to get `config` field definitions, fetches the current
  config (`get_config`) for saved values, renders `.zen-*` form controls dynamically,
  and on Save writes values back via `save_config` — which emits `zenith:config-updated`
  so the bar re-lays-out and the widget picks up the new values.
- **No per-widget config UI code.** The generic window handles all types. To add a new
  configurable widget, just add a `config` block to its manifest — no TS/Rust changes.

#### Owner table (single home — do not create a second)

| Concern | Owner |
|---|---|
| Manifest `config` field definition (Rust) | `widgets/manifest.rs::WidgetConfigField` |
| Manifest `config` field definition (TS) | `shared/types.ts::WidgetConfigField` |
| Stored config values | `config/model.rs::WidgetsConfig.config` + `shared/types.ts::WidgetsConfig.config` |
| Config window (generic) | `src/windows/widget-config/main.ts` + `src/windows/widget-config/widget-config.html` |
| Config window creation | `commands.rs::open_widget_config` |
| Gear button rendering | `manager/main.ts::buildCard` |

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
  `ws-`. Menu actions are handled in `handle_menu_event()` **entirely in Rust** — rename and
  delete open a small unified Tauri dialog window directly (see §13.10). State-change actions
  (create, move, switch, pin) emit typed events that the frontend listens for.
- The rename and delete flows go through `show_dialog(spec)` in `commands.rs` (a single command
  that opens the unified `src/windows/dialog/dialog.html` window). The spec selects which body builder runs.
  This avoids the duplication that existed when there was a `rename.html` + `delete.html`.
- **No frontend event round-trip for rename/delete.** Both open the dialog window directly from
  the Rust menu handler via `spawn`. This prevents double dialogs caused by Tauri event-listener
  accumulation on re-layout (which we observed previously).
- Data is passed to the dialog window via a single `Mutex<DialogSpec>` in Rust (`DIALOG_STATE`)
  retrieved by the `get_dialog_data` IPC command — NOT via query params (Tauri strips query
  strings from `WebviewUrl::App` URLs). See §13.10.
- **Never use `prompt()`, `confirm()`, or `alert()`** in widget JS or the bar itself. Use the
  unified dialog (see §13.10) for any user input or confirmation.
- The right-clicked desktop ID is stored in `WS_CONTEXT_ID` (an `AtomicU32` in `commands.rs`).
- Follow this pattern for any new widget that needs a custom right-click menu:
  1. Add a `show_<name>_context_menu` command in `commands.rs` that calls `build_<name>_menu()`.
  2. Add menu ID constants and extend `handle_menu_event()`.
  3. For actions that need user input/confirmation, call `show_dialog(DialogSpec { kind, data })`
     directly from the Rust menu handler — no frontend event — see §13.10.
  4. For pure state-change actions, emit a typed event the widget JS listens for.
  5. In the widget's JS, prevent default contextmenu and `invoke("show_<name>_context_menu")`.

### 9.6 Special rules

- **`workspace` widget:** shows one circle per virtual desktop; active desktop is a
  filled/colored circle, others outlined. Click to switch. **If there is only one
  virtual desktop, the bar layout layer (`layoutBar`) hides the widget automatically
  — even if the user enabled it.** Enumeration and operations (switch / create / destroy /
  rename / move-window / pin) go through the [`winvd`](https://docs.rs/winvd) crate, which
  wraps the undocumented `IVirtualDesktopManagerInternal` and `IVirtualDesktop` COM interfaces.
  Requires Windows 11 24H2 (build ≥ 26100.2605); see §1.

### 9.7 winvd usage & workspace event sourcing (critical)

**Single source of truth for workspace changes.** The `winvd` crate delivers virtual-desktop
events (`DesktopChanged`, `DesktopCreated`, `DesktopDestroyed`, `DesktopNameChanged`) via a
background thread (`winvd::listen_desktop_events`). **This listener is the ONLY emitter of
`zenith:workspace-changed`.** Do NOT emit `zenith:workspace-changed` from command handlers
(`switch_workspace`, `create_desktop`, `delete_desktop`, `rename_desktop`,
`move_window_to_desktop`). Doing so double-fires the event: once from the command, once from
the COM notification.

- `winvd` functions (`switch_desktop`, `create_desktop`, `remove_desktop`, `get_desktop`,
  `move_window_to_desktop`, `pin_window`, `unpin_window`, `is_pinned_window`) are synchronous
  and return `Result`. They wrap `IVirtualDesktopManagerInternal`/`IVirtualDesktop` COM calls.
- The listener runs on its own thread, receives events via `mpsc::channel`, and emits
  `zenith:workspace-changed` with the new active desktop index.
- **Do not poll for workspace state.** Use the event. The initial emit in `lib.rs::setup` primes
  the bar; subsequent changes come exclusively from the listener.

**Foreground window tracking for move/pin.** The bar window steals focus when
right-clicked, so `GetForegroundWindow()` at menu-open time returns the bar,
not the target app window. Solution: install a Win32 `SetWinEventHook` for
`EVENT_SYSTEM_FOREGROUND` on a dedicated message-pumping thread
(`WINEVENT_OUTOFCONTEXT`). The hook PID-filters our own windows, **walks each
event HWND up to its top-level owner** via `GetAncestor(GA_ROOTOWNER)` (the
event can deliver a child HWND for WinUI/Chromium apps), and rejects non-
`OBJID_WINDOW` events. It stores the last "real" foreground `HWND` in
`workspace::foreground::last_real_foreground_ptr()`. `move_window_to_desktop`
and `toggle_pin_window` call `get_cached_foreground_hwnd_ptr()` which
**prefers a live PID-filtered `GetForegroundWindow()` read** and falls back to
the hook cache only when the live read returns null (foreground was stolen by
the bar). See §13.11.

**Duplicate menu event guard.** Tauri 2's `on_menu_event` can fire twice for a single click.
Guard all workspace menu actions (`WS_CREATE`, `WS_MOVE_HERE`, `WS_MOVE_TO_*`, `WS_TOGGLE_PIN`,
plus `WS_RENAME`/`WS_DELETE` which use `DIALOG_IN_FLIGHT`) with an atomic `AtomicBool`
claim/release pattern. **Important:** release must be deferred (`spawn` a 400 ms timer) — if
the guard is released synchronously, the duplicate event (which arrives microseconds later)
sees an unclaimed guard and re-runs the action (creating two desktops). Synchronous release
is fine only when the duplicate arrives *after* the action completes (e.g. dialog creation
finishes naturally).

**Workspace event deduplication.** `winvd`'s COM notification can deliver overlapping
events for one action (`DesktopCreated` + `DesktopChanged` for one create), and may repeat
on edge cases. The listener in `setup_events` dedupes by `(event-kind, index)` within a
150 ms window — only the first matching emit goes out, duplicates are silenced. This keeps
`zenith:workspace-changed` to exactly one fire per user action.

**Command signatures.** Workspace commands take only their payload (e.g. `id: u32`,
`name: String`) — **no `AppHandle`**. The `winvd` event listener owns all `zenith:workspace-changed`
emits. This keeps command handlers pure and testable.

**Threading.** `winvd` requires COM initialized (`COINIT_APARTMENTTHREADED`) on the calling
thread. `lib.rs::setup` does this once on the main thread before any workspace command runs.
The event listener thread and the foreground hook thread each initialize COM internally
(via `winvd`'s internal handling or explicit `CoInitializeEx` if needed).

**Version requirement.** `winvd` 0.0.49+ requires Windows 11 24H2 (build ≥ 26100.2605). The app
exits with an error on older builds (see §1). Do not add fallback logic — the crate handles
`ERROR_OLD_WIN_VERSION` and propagates it as `winvd::Error`.

### 9.8 Arrange mode — single shared module (no duplication)

**Arrange mode** lets the user add/remove/reorder widgets on the bar. It is activated by
**long-pressing the bar** OR **opening the Widget Manager**. While active, every widget on the
bar and every card in the manager plays a smooth sway animation (`transform`-only, GPU-composited),
shows a round action button (green `+` to add, red `−` to remove), and the bar's three zones
show dashed drop-target borders that highlight on drag-over.

**All arrange logic lives in ONE module: `src/shared/widget-arrange.ts`.** Both `bar/main.ts`
and `manager/main.ts` import from it. **Never duplicate** widget-manipulation logic (add /
remove / move / drag-drop / arrange-state) into a window's `main.ts` — add it to the shared
module and import it.

Public API of `widget-arrange.ts`:

| Export | Used by | Purpose |
|---|---|---|
| `isArrangeActive()` | both | read arrange state |
| `setArrangeActive(active, broadcast?)` | both | flip state + toggle `body.is-arranging` + emit `zenith:arrange-mode` (broadcast=false used by the sync listener to avoid an emit loop) |
| `toggleArrangeMode()` | bar | convenience for long-press |
| `onArrangeChange(fn)` | bar | re-apply chrome when arrange flips |
| `initArrangeSync()` | both | listen for `zenith:arrange-mode` from the other window |
| `addWidget(cfg, id, zone?)` | manager | append to `enabled`, persist, emit `config-updated` |
| `removeWidget(cfg, id)` | both | filter out of `enabled`, persist |
| `moveWidget(cfg, id, zone)` | bar | set zone + reorder `enabled` so widget lands after the last widget in that zone |
| `createWidgetActionBtn(type, handler)` | both | round green `+`/red `−` button factory |
| `attachLongPress(el, cb, ms?)` | bar | pointer-based long-press recognizer |
| `applyArrangeUI(bar, cfg)` | bar | idempotent: adds/removes `−` buttons + `draggable` on every `.widget-slot` based on current arrange state. Call after **every** `layoutBar`. |
| `setupBarDropZones(bar, cfg)` | bar | delegated HTML5 drag-over/drop on the bar; calls `moveWidget` on drop |

Rules:
1. **One module, one concern.** `widget-arrange.ts` owns arrange state + widget config ops +
   the DOM helpers for arrange chrome. Do not split it; do not copy it.
2. **`applyArrangeUI` is idempotent and must run after every layout.** `layoutBar` clears the
   bar DOM, so the action buttons vanish — re-apply in the `configUpdated` listener.
3. **Cross-window state uses the `zenith:arrange-mode` event**, not a shared file. The manager
   calls `setArrangeActive(true)` on open and `setArrangeActive(false)` in `beforeunload`.
4. **Widget config changes go through `saveConfig()`** which emits `zenith:config-updated`; the
   bar listens and re-lays-out. Never emit a separate "widget added" event.
5. **Sway + drop-zone styling is in `src/styles/arrange.css`**, imported by BOTH
   `bar-globals.css` and `globals.css`. Keep it transform-only (no width/height/top/left).
6. **Cross-window drag-and-drop (manager → bar) uses pointer events + Tauri
   event sync**, NOT HTML5 DnD (which is impossible across isolated webviews).
   - Manager side: `attachCrossDragSender(card, id)` arms a card (only if the
     widget is NOT already on the bar). After a 6px movement threshold a
     "faked" ghost (`.zen-cross-ghost`) follows the cursor inside the manager
     window and emits `zenith:cross-drag-start { id }`. On `pointerup` it
     emits `zenith:cross-drag-end`.
   - Bar side: `setupBarReceiveDrop(bar, cfg)` listens for `cross-drag-start`
     → adds `.is-receiving` to the bar (zone drop indicators appear) → on
     `pointermove` highlights the zone under the cursor (`.is-drop-target`)
     → on `pointerup` over a zone calls `addWidget(cfg, id, zone)` (the real
     widget loads). `cross-drag-end` clears the receiving state if the drop
     landed outside the bar.
   - Enabled widgets are NOT draggable from the manager (the sender is only
     attached to not-enabled cards, marked `.widget-card.is-draggable`).

---

## 10. Window, base HTML & permissions contract

- **Shared base HTML.** All windows (`src/windows/bar/index.html`, `src/windows/settings/settings.html`, `src/windows/manager/widgets.html`, `src/windows/dialog/dialog.html`, the popups, and the window shells co-located in `widgets/<id>/window/`) use the
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

### 13.10a WebView2 white-flash prevention

Transparent Tauri windows that use Win32 Acrylic/Mica via
`SetWindowCompositionAttribute` race the WebView2 first paint by default:
the WebView paints a white background before DWM blur is registered,
producing a visible white flash before the blur settles.

Fix is two-layered:
1. `additional_browser_args("--default-background-color=00000000")` on
   every transparent `WebviewWindowBuilder` (and `additionalBrowserArgs`
   in `tauri.conf.json` for the bar window). Format is `0xAABBGGRR`
   (alpha-blue-green-red, little-endian). This sets the WebView's
   background to fully transparent BEFORE the first paint.
2. Build the window with `visible(false)`. After `.build()` returns,
   call `apply_fixed_acrylic` (or `apply_material` for the bar).
   Finally show the window via
   `SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE)`
   followed by `win.set_focus()`. The window is hidden while the
   materials are registered, so the WebView paints nothing visible —
   when the `SWP_SHOWWINDOW` flips the window visible, DWM blur is
   already in place.

This pattern must be used for **every** transparent window: bar,
settings, widgets, dialog. See `commands.rs::create_*_window`.

### 13.10b `SetWindowPos` show flags: drop `SWP_NOACTIVATE` and keep `SWP_NOSIZE | SWP_NOMOVE`

Two hard-won bugs live in the `SetWindowPos(...)` call every transparent
window uses to reveal itself after the material has been applied:

1. **Geometry bug.** `SetWindowPos` interprets the `cx`/`cy` parameters as
   the new window size **only when `SWP_NOSIZE` is absent**, and `x`/`y`
   as the new position **only when `SWP_NOMOVE` is absent**. The common
   "show window, don't touch geometry" call passes `0, 0, 0, 0` for the
   position/size args — so without `SWP_NOSIZE | SWP_NOMOVE` the window is
   silently **resized to 0×0 and moved to (0,0)**. This bug caused the
   settings and widgets windows to open blank / appear frozen (JS ran to
   completion and logged `settings ready`, but the window had zero area so
   nothing was visible). The dialog window *appeared* to work because
   `mountDialog` → `fitWindow()` calls `getCurrentWindow().setSize()` after
   two `requestAnimationFrame`s, which recovered the 0×0 size — but that
   two-frame + IPC round-trip was exactly the content-show delay users saw.

2. **Focus bug.** `SWP_NOACTIVATE` tells `SetWindowPos` "show without
   activating". Combined with the trailing `win.set_focus()` this RACES
   Windows' foreground-window restriction rules: `SetForegroundWindow`
   can be silently rejected if our process didn't recently have
   foreground. Result: the popup window appears but is **not focused** —
   keyboard input goes nowhere. Drop `SWP_NOACTIVATE` so the window is
   properly activated on show. `set_focus()` then acts as a safety net,
   not a race.

**Correct (every transparent window show, bar/popup/settings/widgets/dialog/widget-config):**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, SWP_SHOWWINDOW, SWP_NOZORDER,
    SWP_NOSIZE, SWP_NOMOVE,
};
let hwnd = win.hwnd().map_err(|e| e.to_string())?;
let _ = unsafe {
    SetWindowPos(hwnd, None, 0, 0, 0, 0,
        SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE)
};
let _ = win.set_focus();
```

**Wrong** (resizes to 0×0, moves to top-left):
```rust
SetWindowPos(hwnd, None, 0, 0, 0, 0,
    SWP_SHOWWINDOW | SWP_NOZORDER)  // missing SWP_NOSIZE | SWP_NOMOVE
```

**Wrong** (appears but doesn't take foreground):
```rust
SetWindowPos(hwnd, None, 0, 0, 0, 0,
    SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE)
```

Rule: any `SetWindowPos` call that only wants to change visibility or
Z-order MUST pass `SWP_NOSIZE | SWP_NOMOVE` whenever the x/y/cx/cy args
are zero/unused. And it MUST NOT pass `SWP_NOACTIVATE` if the window is
expected to receive input immediately on open (which is every popup in
this app — volume, calendar, settings, widgets, dialog, widget-config).

### 13.10 Unified dialog window for user input / confirmation

The bar window is only 40px tall with `overflow: hidden`. JS built-in dialogs (`prompt`,
`confirm`, `alert`) are rendered by the WebView and are clipped by the window bounds — the
user sees nothing and the app appears frozen.

- **Never call `prompt()`, `confirm()`, or `alert()`** in widget JS or the bar itself.
- Zenith has **one** transient dialog window for all user input / confirmation flows. It
   loads `src/windows/dialog/dialog.html` and renders a builder from a registry selected by `kind`. New dialogs
  add a builder; they do **not** add a new HTML/JS bundle.
- The window is created by `show_dialog(spec)` in `commands.rs::show_dialog`, called directly
  from the menu handler via `spawn`, with `decorations: false`, `transparent: true`,
  `apply_fixed_acrylic`, `set_rounded_corners`, `max_inner_size(600, 600)`, `resizable: false`.
- Data is passed from Rust to the dialog via a `Mutex<DialogSpec>` (`DIALOG_STATE` in
  `commands.rs`) and retrieved by the dialog JS through the IPC command `get_dialog_data`.
  The dialog window calls `show_dialog` with `kind: "rename"` or `kind: "delete"` and `data` is
  the payload the builder consumes (`[id, current_name]` for rename, `id` for delete).
- The dialog window mounts via `mountDialog(opts)` in `src/windows/dialog/base.ts`. It always
  uses `mountWindow()` for the chrome (header + close button + drag) and optionally renders a
  footer (action buttons) and a body (HTMLElement or builder). If `actions` is empty/omitted,
  the footer is not rendered.
- **No frontend event round-trip.** The menu handler invokes `show_dialog(spec)` directly in
  Rust (no `app.emit` for dialog opens) to prevent double dialogs caused by Tauri
  event-listener accumulation on re-layout.
- Buttons use `.zen-button` variants: primary / outline / ghost / destructive. The Delete
  button uses `.is-destructive`. For "borderless" text-link buttons, set
  `action.borderless = true`.
- Built-in builders live in `src/windows/dialog/builders.ts`. To add a new dialog:
  1. Define a builder function `myDialogBuilder(data): DialogOptions` in `builders.ts` and
     register it via `registerDialog("my_kind", myDialogBuilder)`.
  2. From Rust, call `show_dialog(DialogSpec { kind: "my_kind".into(), data: Some(...) })`.

#### `mountDialog` API (`src/windows/dialog/base.ts`)

```ts
import { mountDialog, type DialogOptions, type DialogAction } from "@/windows/dialog/base";

await mountDialog({
  title: "Delete Desktop",                      // header text (always shown)
  data: { id: 0 },                              // opaque payload, available as ctx.data
  body: (ctx) => { /* returns HTMLElement */ }, // scrollable content; omit → no body
  actions: [                                     // omit / empty → NO footer rendered
    { label: "Cancel", variant: "outline", onClick: (ctx) => ctx.close() },
    { label: "Delete", variant: "destructive", autofocus: true,
      onClick: async (ctx) => { /* return false to keep open */ return true; } },
  ],
  disableContextMenu: true,                      // default true in prod
  disableSelect: true,                           // default true
  closeOnEscape: true,                           // default true
  onKeyDown: (e, ctx) => { /* return false to preventDefault */ },
});
```

- `DialogAction.variant`: `"primary" | "secondary" | "outline" | "ghost" | "destructive" | "danger" | "alert" | "info" | "success"`. `danger`/`alert` map to `is-destructive`.
- `DialogAction.borderless: true` renders a plain text link (no background / no border / minimal padding).
- `DialogAction.autofocus: true` focuses this button after mount (only the first one wins).
- `DialogAction.onClick(ctx)` may return `false` (or `Promise<false>`) to keep the dialog open; anything else closes it.
- `Enter` (when focused on an action button) fires its `onClick` unless `submitOnEnter: false`.
- `Escape` closes (unless `closeOnEscape: false`).
- The window auto-fits its content (≤ 600×600), with `overflow: auto` on the body wrapper. Layout is `flex column` with header / scrollable content / footer; the footer is rendered only when `actions.length > 0`.
- **Permissions** for the dialog window are granted in `src-tauri/capabilities/dialog.json`. Includes `core:window:allow-set-size` so the dialog can resize itself. Matches all windows whose label starts with `dialog-` (i.e., `dialog-rename`, `dialog-delete`, etc.).

### 13.11 Foreground HWND for move/pin operations

`GetForegroundWindow()` returns the wrong window in two distinct failure modes:

1. **After right-click** the bar's webview child briefly steals Windows
   foreground — `GetForegroundWindow()` then returns our own PID, which the
   move/pin COM call rejects.
2. **`EVENT_SYSTEM_FOREGROUND` can deliver a child HWND** (WinUI XAML islands,
   Chromium/Electron content windows) for the new foreground — the COM virtual
   desktop API requires a *top-level* HWND and silently rejects child HWNDs.

Fix (three layers, see `src-tauri/src/workspace/foreground.rs`):

- A Win32 `SetWinEventHook` for `EVENT_SYSTEM_FOREGROUND` runs on a dedicated
  message-pumping thread (required for `WINEVENT_OUTOFCONTEXT`). It PID-filters
  our own windows and walks each event HWND up to its **top-level owner**
  (`GetAncestor(hwnd, GA_ROOTOWNER)`) so the cache never holds a child HWND.
  It also rejects events where `idObject != OBJID_WINDOW (0)` — the same
  foreground change can fire multiple times with `OBJID_CLIENT`,
  `OBJID_SYSMENU`, etc., and those carry child HWNDs that confuse the COM API.
- The hook is **seeded** with `GetForegroundWindow()` at install time so the
  "Move Window To" submenu is available the first time the user right-clicks.
- `workspace::commands::get_cached_foreground_hwnd_ptr` calls
  `foreground::best_effort_foreground_ptr()` which **prefers a live
  PID-filtered `GetForegroundWindow()` read** and falls back to the hook cache
  only when live returns null (foreground was stolen by our own bar). This
  guards against hook thread death and stale-cache races.

### 13.12 Restart must unregister the AppBar before spawning

Spawning a new process and exiting the old one leaves a brief overlap where both windows exist.
If the old AppBar is still registered, the new process can't claim the band and the user sees
two bars.

- **Correct:** `unregister_appbar` → `spawn` new exe → `app.exit(0)`.
  See `src-tauri/src/commands.rs:handle_menu_event`, `MI_RESTART`.

### 13.13 Explorer restart must re-register the AppBar

When `explorer.exe` crashes and restarts, it broadcasts the registered window message
`"TaskbarCreated"` on the message-only-window scope. The new explorer instance has no
knowledge of AppBars registered before it started → the work-area reservation is lost and
maximized windows cover the bar.

- **Detected via** a hidden message-only window (`HWND_MESSAGE`) on a dedicated thread
  (`window/appbar_monitor.rs`). The window class registers a `WNDCLASSEXW` once and pumps
  messages; on receiving `TaskbarCreated` it emits `zenith:appbar-restore`.
- **Handled via** `lib.rs::setup` → `handle.listen("zenith:appbar-restore", ...)` calls
  `window::register_appbar(&bar)` again. The re-registration is idempotent (`ABM_NEW` is
  safe to issue again; the shell treats it as a refresh).
- Without this, killing explorer (or letting it crash) leaves the bar visible but no longer
  reserving space, so maximized windows cover it.

### 13.14 Popup positioning must clamp to the target monitor

Any popup-style window anchored to a bar widget — volume slider, calendar, future tooltips,
menus, etc. — **must** clamp its final position to the monitor that contains the
widget before calling `WebviewWindowBuilder::position(...)`. The single
implementation lives in `src-tauri/src/window/monitor.rs::clamp_to_monitor`
(re-exported as `window::monitor::clamp_to_monitor` / `window::clamp_to_monitor`).

**Why:** `position()` does no clamping. On a multi-monitor setup (or any single
monitor whose origin is not `(0, 0)` and any DPI scale ≠ 100%), naive
positioning can place the popup partially off-screen — the user clicks a widget
and sees a half-window.

**Contract — every popup builder must do this:**

```rust
use crate::window;

// Inside `create_*_window`:
let win_w: i32 = 260;
let win_h: i32 = 60;
let (x, y, w, h) = window::clamp_to_monitor(
    proposed_x, proposed_y, win_w, win_h,
);

tauri::WebviewWindowBuilder::new(app, label, url)
    .inner_size(w as f64, h as f64)
    .position(x as f64, y as f64)
    .build()?;
// then continue with the §13.10a material-after-build sequence
```

- `(proposed_x, proposed_y)` is the **bar-widget anchor** (the absolute OS-pixel
  top-left of the widget that triggered the popup — usually computed by the
  frontend and passed to the IPC command). `clamp_to_monitor` looks up the
  monitor that contains that point, then snaps the popup's `(x, y)` so the
  window is fully inside that monitor's `rcWork`.
- The function preserves the requested `w`/`h`; if the size itself doesn't
  fit a tiny work area, the work-area size is returned (callers should set a
  `max_inner_size` that matches).
- The helper works at the **(x, y) level**, not at the `HWND` level — it
  resolves the owning monitor before any window is created, so it suits the
  pre-builder position flow. There is also a convenience
  `clamp_rect_to_monitor(RECT) -> RECT`.
- **`appbar.rs::monitor_of`** stays private to the AppBar domain. Do NOT
  reuse it for popups — AppBar placement intentionally covers the full
  monitor width without clamping.
- Already-applied: volume popup (`src-tauri/src/volume/commands.rs::create_volume_popup_window`).
  New: calendar popup, see `src-tauri/src/calendar/commands.rs::create_calendar_window`.

---

## 14. Logging & performance diagnostics

Zenith writes per-window logs to `%TEMP%/zenith/{YYYY-MM-DD}/{window}.log`, organized by date
so logs from different sessions don't overwrite each other. Use them to diagnose memory, startup
time, and IPC issues.

### 14.1 Log files

| File | Source |
|---|---|
| `%TEMP%/zenith/{date}/bar.log` | bar window (`src/windows/bar/index.html`) |
| `%TEMP%/zenith/{date}/settings.log` | settings window (`src/windows/settings/settings.html`) |
| `%TEMP%/zenith/{date}/widgets.log` | widget manager (`src/windows/manager/widgets.html`) |
| `%TEMP%/zenith/{date}/dialog.log` | unified dialog window (`src/windows/dialog/dialog.html`) |

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
