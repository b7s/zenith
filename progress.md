# Zenith — Progress & Roadmap

> Living document. Update status as work completes.

---

## Done — Foundation (scaffolding)

| Area | Status | Notes |
|---|---|---|
| Project structure | ✅ | Tauri 2 + Rust 2021 + plain TS/CSS, DDD layout |
| AGENTS.md contract | ✅ | 14 sections, all conventions codified |
| Config domain (Rust) | ✅ | Safe getter, 6 tests pass, `#[serde(default)]` everywhere |
| Config client (TS) | ✅ | `loadConfig()`, `getConfigValue()`, `saveConfig()` |
| Icon system | ✅ | Static Lucide registry + Windows font fallback, sync API |
| Window chrome | ✅ | `mountWindow()`, custom header, drag region, theme bootstrap |
| CSS library | ✅ | tokens (shadcn oklch) + base + components + icons + window |
| Base HTML (3 windows) | ✅ | bar, settings, widgets — shared skeleton |
| Transparency | ✅ | Mica/Acrylic via `SetWindowCompositionAttribute` |
| AppBar | ✅ | `SHAppBarMessage` work-area reservation |
| System tray | ✅ | Context menu (Settings/Widgets/Restart/Close) |
| Build pipeline | ✅ | `npm run check` / `cargo check` clean, git pushed |
| Dialog system | ✅ | Unified dialog window (`dialog.html` + `mountDialog` + builder registry); no `prompt`/`confirm`/`alert` |
| AppBar explorer-restart monitor | ✅ | Message-only window detects `TaskbarCreated`, re-registers AppBar automatically |
| Workspace domain | ✅ | `winvd`-based virtual desktop management; rename, delete, create, switch via unified dialog; move/pin gated off |
| Window creation pattern | ✅ | `visible(false)` → apply material → `set_disable_transitions(DWMWA_TRANSITIONS_FORCEDISABLED)` → `SetWindowPos` with `SWP_NOSIZE\|SWP_NOMOVE` → `set_focus()` (white-flash + OS animation fix); synchronous `data-theme` via inline `<script>` (no theme flash) |

---

## Phase 1 — Visible Bar (DONE)

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
- [x] `.zen-checkbox` — Apple-style toggle switch: label + optional description on the left, switch knob on the right
- [x] `.zen-radio-group` + `.zen-radio-card` — new: card-style radio buttons
- [x] `.zen-tabs` + `.zen-tab` (`.is-active`) — new: horizontal tab strip
- [x] `.zen-color-field` — new: swatch + hex display
- [x] `.zen-section` / `.zen-divider` — new: layout grouping
- [x] `.zen-window__content` — translucent 30% background for unified surface
- [x] `.zen-icon-button:hover` — translucent accent background

---

## Phase 2 — Settings Window (IN PROGRESS)

Goal: full settings form, changes apply live.

- [x] Tabbed layout with reusable `mountTabs()` in `src/shared/tabs.ts`
- [x] **Bar tab** (dual-purpose): material, tint alpha, background mode/colors, corner radius, bar height, margins, padding, theme — all in one tab
- [x] **About tab**: version, name, description, logo, GitHub link
- [x] "Widgets" external link button in tab bar → opens widget manager window
- [x] All fields bound to config, `saveConfig()` on change, live reload bar
- [x] Use `.zen-*` component classes exclusively
- [x] Color pickers (HTML `<input type="color">` in settings)
- [x] Widget position per widget (left/center/right) — drag between bar zones in arrange mode
- [x] Widget reordering (drag-and-drop within bar, +/− from widget manager)

---

## Phase 3 — Widget Manager (DONE)

Goal: browse/add/remove widgets with the arrange-mode UX.

- [x] Grid of widget cards (icon + name + description) from manifest scan
- [x] Green `+` to add, red `−` to remove — round action buttons on each card
- [x] Search input filters the grid (manager header search wired)
- [x] Arranging: opening manager OR long-pressing the bar activates arrange mode
- [x] Sway animation on widgets in both bar and manager (GPU-composited `transform`)
- [x] Bar drop-zone indicators: dashed borders on left/center/right, highlighted on drag-over
- [x] Drag-and-drop to reorder widgets between bar zones (HTML5 DnD, delegated)
- [x] Persists to `config.widgets.enabled` + `positions` via `saveConfig()` → live bar reload
- [x] Cross-window drag-and-drop (manager → bar) fixed: pointer capture + screen coordinate sync via Tauri events, belt-and-suspenders fallback

---

## Phase 4 — Core Widgets

- [x] **Battery** — Win32 `GetSystemPowerStatus`, icon variants (warning/low/medium/full/charging), hover tooltip shows percent + charging state
- [x] **Volume** — system audio endpoint via `IAudioEndpointVolume` (Win32 API); icon changes with level/mute; scroll to adjust; right-click mute/unmute; click opens acrylic popup with `.zen-slider`; hover tooltip shows percent
- [x] **Workspace** — virtual desktop dots (filled = active), click to switch; auto-hide if only 1 desktop; rename, delete, create via unified dialog; move/pin gated off (pending foreground HWND fix)
- [x] **Date & Time** — configurable timezone (IANA), 12/24h format, show/hide date; generic widget-config window with gear button in Widget Manager; click widget → opens Apple-style acrylic calendar popup (transparent, click-outside-to-close, prev/next month, year `<select>` last 30 years, expands as user navigates)
- [x] **Shutdown** — power action popup (shutdown/restart/sleep/hibernate/lock/logout) with two-step confirmation, translucent buttons, header close button via `mountWindow`; Lock (`LockWorkStation`), Logout (`EWX_LOGOFF`); uses `set_disable_transitions` + inline theme sync for instant open
- [x] **System stats** — CPU%, GHz, RAM, GPU, HD, network throughput; three visual styles (bar/dots/graph); configurable via widget-config window

---

## Phase 5 — Polish

- [x] Native right-click context menu on bar (Win32 `popup_menu` API, `show_context_menu` / `show_workspace_context_menu` commands)
- [x] Popup focus fix — 500ms delay before `set_focus()` on all popup windows to beat Windows foreground rules
- [x] Instant popup open — `DWMWA_TRANSITIONS_FORCEDISABLED` disables OS fade animation; `.zen-window__content` CSS fade-in (120ms) provides smooth entry without lag
- [ ] Custom CSS injection (`%APPDATA%\zenith\custom.css`, hot-reload)
- [ ] Multi-monitor support (config schema supports `MonitorsSelection`, AppBar handles monitor-of; frontend UI missing)
- [ ] Motion domain (model/config exists in `MotionConfig`; runtime domain not yet wired)
- [ ] Auto-start on login
- [ ] Installer (NSIS bundle)

---

## Architecture notes

- **Widget loading**: widgets are plain HTML/CSS/JS in `widgets/<name>/`. The bar loads them via Tauri's asset protocol or by reading files and injecting into the DOM. Start simple: inject HTML string into a container, load widget JS as a module.
- **Config is the only state**: widget enabled list + positions live in `config.json`. The bar re-reads on `zenith:config-updated` event and re-renders.
- **No framework**: bar layout is plain DOM manipulation. `layoutBar()` clears zones and re-appends widget slots.
