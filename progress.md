# Zenith — Progress & Roadmap

> Living document. Update status as work completes.

---

## Done — Foundation (scaffolding)

| Area | Status | Notes |
|---|---|---|
| Project structure | ✅ | Tauri 2 + Rust 2021 + plain TS/CSS, DDD layout |
| AGENTS.md contract | ✅ | 12 sections, all conventions codified |
| Config domain (Rust) | ✅ | Safe getter, 6 tests pass, `#[serde(default)]` everywhere |
| Config client (TS) | ✅ | `loadConfig()`, `getConfigValue()`, `saveConfig()` |
| Icon system | ✅ | Static Lucide registry + Windows font fallback, sync API |
| Window chrome | ✅ | `mountWindow()`, custom header, drag region, theme bootstrap |
| CSS library | ✅ | tokens (shadcn oklch) + base + components + icons + window |
| Base HTML (3 windows) | ✅ | bar, settings, widgets — shared skeleton |
| Transparency | ✅ | Mica/Acrylic via `SetWindowCompositionAttribute` |
| AppBar | ✅ | `SHAppBarMessage` work-area reservation |
| System tray | ✅ | Context menu (Settings/Widgets/Restart/Close) |
| Build pipeline | ✅ | `make check` clean, git pushed |

---

## Phase 1 — Visible Bar (IN PROGRESS)

Goal: `npm run tauri dev` shows a real top bar with a working clock widget.

### 1.1 Bar window UI layout
- [x] Build the bar strip DOM: `[data-bar]` → `.bar-zones` → `.bar-zone--left / --center / --right`
- [x] CSS for full-width, fixed-height (from config `bar_height`), flexbox zones
- [x] Read config to size the bar (`appearance.bar_height`) and apply margins
- [x] No background on body (transparency contract) — zones are transparent containers

### 1.2 Widget system — Rust backend (`widgets/` domain)
- [x] `manifest.rs` — parse `widgets/<name>/manifest.json` (name, id, version, default_zone, icon, min_width)
- [x] `registry.rs` — scan `widgets/` dir at startup, collect manifests, expose `list_widgets() -> Vec<WidgetManifest>`
- [x] `commands.rs` — `#[tauri::command] get_widgets() -> Vec<WidgetManifest>` for the manager UI
- [x] Resolve widget HTML/JS/CSS paths relative to the widget folder

### 1.3 Widget system — frontend loader (`src/shared/widgets.ts`)
- [x] `loadWidgets()` — call `get_widgets` IPC, return manifest list
- [x] `renderWidget(manifest, zone)` — inject widget HTML into a zone element
- [x] `layoutBar(config)` — read `config.widgets.enabled` + `positions`, place each widget in its zone in order
- [x] Widget sandbox: each widget gets its own container `.widget-slot` with `min_width`

### 1.4 Clock widget (`widgets/clock/`)
- [x] `manifest.json` — `{ id: "clock", default_zone: "left", icon: "clock", min_width: 80 }`
- [x] `widget.html` — minimal markup (`<span class="clock-time">`)
- [x] `widget.js` — update time every second, format `HH:MM`
- [x] `widget.css` — font, color from tokens, padding

### 1.5 Verify
- [x] `npm run tauri dev` shows bar at top with clock in left zone
- [x] Bar height matches config
- [x] Transparency/Mica visible
- [x] AppBar reserves space (maximize a window — it stops below the bar)

### 1.6 Modern glass controls (components.css)
- [x] `.zen-card` — translucent `color-mix` background (75% opacity) for glassmorphism
- [x] `.zen-input` — translucent background (55% opacity) to let acrylic show through
- [x] `.zen-textarea` — new: translucent, resizable, same style as input
- [x] `.zen-button` — translucent primary, translucent outline/ghost hover states
- [x] `.zen-button.is-lg` — new: larger variant
- [x] `.zen-select` + `.zen-select-wrapper` — new: styled native select with chevron
- [x] `.zen-slider` — new: custom range slider with themed thumb
- [x] `.zen-switch` — new: toggle switch with animated thumb
- [x] `.zen-checkbox` — new: styled checkbox with checkmark clip-path
- [x] `.zen-radio-group` + `.zen-radio-card` — new: card-style radio buttons
- [x] `.zen-tabs` + `.zen-tab` (`.is-active`) — new: horizontal tab strip
- [x] `.zen-color-field` — new: swatch + hex display
- [x] `.zen-section` / `.zen-divider` — new: layout grouping
- [x] `.zen-window__content` — translucent 30% background for unified surface
- [x] `.zen-icon-button:hover` — translucent accent background

---

## Phase 2 — Settings Window (IN PROGRESS)

Goal: full settings form, changes apply live.

- [x] Tabbed layout: Appearance · Bar · Widgets · About (reusable `mountTabs()` in `src/shared/tabs.ts`)
- [x] **Appearance tab**: material (acrylic/mica/none), tint alpha slider, corner radius, theme (auto/dark/light)
- [x] **Bar tab**: bar height slider, margins (top/left/right), background mode (transparent/solid/gradient), color swatch
- [x] **Widgets tab**: enabled list (checkbox per widget from manifest)
- [x] **About tab**: version, name, description
- [x] All fields bound to config, `saveConfig()` on change, live reload bar
- [x] Use `.zen-*` component classes exclusively
- [ ] Color pickers: native `ChooseColor` Win32 dialog integration
- [ ] Widget position per widget (left/center/right selector)
- [ ] Widget reordering (drag or up/down buttons)

---

## Phase 3 — Widget Manager

Goal: browse/add/remove widgets with thumbnails.

- [ ] Grid of widget cards (thumbnail + name + description)
- [ ] Green ✓ = add to enabled, red − = remove
- [ ] Search input filters the grid
- [ ] Reordering (drag or up/down arrows)
- [ ] Persists to `config.widgets.enabled` + `positions`

---

## Phase 4 — Core Widgets

- [ ] **Battery** — % + charging state + icon
- [ ] **Volume** — current level + icon, click to open system mixer
- [ ] **Workspace** — virtual desktop dots (filled = active), click to switch; auto-hide if only 1 desktop (requires `IVirtualDesktopManagerInternal` COM)
- [ ] **System stats** — CPU/RAM mini graphs

---

## Phase 5 — Polish

- [ ] Native right-click context menu on bar (Win32, not HTML)
- [ ] Custom CSS injection (`%APPDATA%\zenith\custom.css`, hot-reload)
- [ ] Multi-monitor support (per-monitor or all)
- [ ] Motion domain (GPU/CPU animation backend selection)
- [ ] Auto-start on login
- [ ] Installer (NSIS bundle)

---

## Architecture notes

- **Widget loading**: widgets are plain HTML/CSS/JS in `widgets/<name>/`. The bar loads them via Tauri's asset protocol or by reading files and injecting into the DOM. Start simple: inject HTML string into a container, load widget JS as a module.
- **Config is the only state**: widget enabled list + positions live in `config.json`. The bar re-reads on `zenith:config-updated` event and re-renders.
- **No framework**: bar layout is plain DOM manipulation. `layoutBar()` clears zones and re-appends widget slots.
