<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="src-tauri/icons/defaults/icon-light.png">
    <source media="(prefers-color-scheme: dark)" srcset="src-tauri/icons/defaults/icon-dark.png">
    <img alt="Zenith logo" width="160" height="160">
  </picture>
</p>

<h1 align="center">Zenith</h1>

<p align="center">A customizable, always-on-top status bar for Windows&nbsp;11, docked to the top edge of your screen.</p>

<p align="center">
  <a href="https://github.com/b7s/zenith/releases/latest">
    <img alt="GitHub release" src="https://img.shields.io/github/v/release/b7s/zenith?style=flat-square&label=version&color=6366f1">
  </a>
  <a href="./LICENSE">
    <img alt="MIT License" src="https://img.shields.io/badge/license-MIT-6366f1?style=flat-square">
  </a>
  <img alt="Windows 11 24H2+" src="https://img.shields.io/badge/Windows%2011%2024H2+-e11d48?style=flat-square&logo=windows11&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-edition%202021-f97316?style=flat-square&logo=rust">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri%202-010409?style=flat-square&logo=tauri">
  <img alt="Vite 8" src="https://img.shields.io/badge/Vite%208-646cff?style=flat-square&logo=vite&logoColor=white">
</p>

<img src="public/bar-example.webp" alt="bar example">

<p align="center">
  <a href="#download">Download</a> &middot; <a href="#build-from-source">Build from source</a> &middot; <a href="#widgets">Widgets</a> &middot; <a href="#configuration">Configuration</a> &middot; <a href="#license">License</a>
</p>

<p align="center">
  <strong>Requires Windows 11 24H2 (build&nbsp;≥&nbsp;26100.2605).</strong>
</p>

---

## What is Zenith?

Zenith is a **top bar for Windows 11** — a custom, always-available status bar that docks to
the top edge of the screen.

Core ideas:

- **Stays on top and reserves space.** Zenith registers as a Windows **desktop AppBar**
  (`SHAppBarMessage`), so the shell shrinks the work area. Maximized windows stop *below* the
  bar and can never cover it — exactly like the native Taskbar.
- **Native transparency.** The bar, Settings, and Widget Manager windows use Windows **Acrylic**
  or **Mica** blur applied through the Win32 `SetWindowCompositionAttribute` accent API. The
  windows are fully transparent; the OS paints the blur.
- **Widget system.** Widgets are small, standalone apps (plain JS/CSS/HTML) living in
  `widgets/`. Each has a `manifest.json`. Users toggle them on/off in the Widget Manager;
  their order and position (left/center/right) are saved to config.
- **Fully customizable visuals.** The Settings window exposes the background mode
  (Acrylic/Mica/Gradient/Solid/None), tint transparency, gradient colors and alphas, bar
  height, padding and margins, corner rounding, theme (dark/light/auto), and monitor
  selection. Changes apply live. Power users may additionally drop a
  `%APPDATA%\zenith\custom.css` that is hot-reloaded.
- **Right-click anywhere empty on the bar** → native context menu: **Settings · Widgets ·
  Restart Bar · Close Bar**.
- **Custom chrome.** No window uses the Windows title bar. Every window has a custom header:
  semi-bold title on the left, `×` close on the right. The Widget Manager header also has a
  search input.
- **Minimal footprint.** Goal is the lowest possible RAM and CPU. No heavy framework, no
  per-window CSS backgrounds, compositor-friendly animations only.

### Virtual desktop support

Zenith drives the virtual-desktop API via the [`winvd`](https://docs.rs/winvd) crate. The
Workspace widget shows one circle per virtual desktop; the active desktop is filled/colored,
others outlined. Click to switch, right-click to rename, delete, or create new desktops. This
requires Windows 11 24H2 (build ≥ 26100.2605) — users on older builds see a startup error and
the app exits.

---

## Download

Pre-built installers are published on the [Releases page](https://github.com/b7s/zenith/releases).

1. Go to <https://github.com/b7s/zenith/releases/latest>.
2. Download the **`Zenith_x.x.x_x64-setup.exe`** installer for the latest release.
3. Run the installer. Zenith is installed for the current Windows user (no admin elevation
   required) and added to **Start → Zenith** in your Start menu.
4. Launch **Zenith**. The bar appears docked at the top of your screen.
5. (Optional) Open **Settings → Updates** to enable **Start with Windows** for auto-launch on
   login.

Zenith runs in the **system tray** — right-click the tray icon to open Settings, open the
Widget Manager, check for updates, or quit.

### Auto-update

Zenith checks for new releases in the background every 24 hours. When a new version is
available, the tray icon shows a badge; click **Check for updates** to download the new
installer. Auto-update can be disabled in Settings → Updates.

---

## Build from source

If you'd rather compile Zenith yourself, you'll need a Windows 11 (24H2+) machine with the Rust
toolchain and Node.js.

### Prerequisites

- [**Rust**](https://www.rust-lang.org/tools/install) (stable, edition 2021) — install via
  `rustup`.
- [**Node.js**](https://nodejs.org/) ≥ 20 (LTS recommended) and `npm`.
- [**Git**](https://git-scm.com/downloads).
- **Windows 11 24H2** (build ≥ 26100.2605) — required by the `winvd` crate.
- The [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  (MSVC) — required by the Rust linker on Windows.

### Clone

```bash
git clone https://github.com/b7s/zenith.git
cd zenith
```

### Install frontend dependencies

```bash
npm install
```

### Run in development mode

```bash
npm run tauri dev
```

This starts Vite (the frontend dev server) on http://localhost:1422 and launches the Tauri app
pointed at it. The bar appears at the top of your screen and hot-reloads on frontend changes.

### Build a production installer

```bash
npm run tauri build
```

The unsigned NSIS installer and the raw executable are written to
`src-tauri/target/release/bundle/`. Run the `*-setup.exe` to install.

> To produce a signed, distributable installer, see the
> [Tauri signing guide](https://v2.tauri.app/distribute/sign/) and supply the
> `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` environment variables.

---

## Widgets

Widgets are the heart of Zenith. Each widget is a self-contained folder in `widgets/<id>/`
with a `manifest.json`, a `widget.html` fragment, an optional `widget.js` IIFE, and an optional
`widget.css`. The Rust backend scans the `widgets/` directory at startup — there is no central
registry to edit. To add a widget, just drop a new folder in `widgets/`; to remove one, delete
its folder.

Open the **Widget Manager** (right-click the bar → **Widgets**, or the tray icon) to toggle
widgets on/off, rearrange them (drag-and-drop across the bar's left / center / right zones), and
configure per-widget settings via the gear button.

### Shipped widgets

| Widget | Description | Default zone | Configurable |
|---|---|---|---|
| **Clock** | Current time display | left | — |
| **Date & Time** | Date and time with configurable timezone, format, and calendar popup | center | yes |
| **Workspace** | Virtual desktop switcher — one circle per desktop; click to switch, right-click to rename/delete/create | left | — |
| **Color Picker** | Eyedropper to sample any pixel; right-click for the full picker window | right | yes |
| **AI Agents** | Real-time status of AI coding agents (Claude Code, Codex, OpenCode) | right | yes |
| **Volume** | System volume control and mute toggle | right | — |
| **Battery** | Battery level and charging status | right | — |
| **Shutdown** | Shutdown, restart, sleep, hibernate from the bar | right | — |
| **Quick Toggle** | Toggle WiFi, Bluetooth, Dark Mode, Focus Assist, Airplane Mode, Night Light | right | yes |
| **Media Control** | Now-playing info with play/pause, next/previous, and seek | left | yes |
| **Weather** | Current weather + 7-day forecast from OpenWeatherMap (click to open forecast, air quality & charts) | right | yes |
| **System Stats** | CPU, RAM, GPU, HD, and network usage with switchable styles (bar/dots/graph) | right | yes |
| **Alarms** | Show upcoming alarms and events; relative or absolute time | right | yes |
| **Git Manager** | Failed CI + open PRs across GitHub, GitLab, and Bitbucket accounts | right | yes |
| **Web Apps** | Open your own links as personal web-app windows | left | yes |

### Adding a widget

Each widget folder follows this layout:

```
widgets/<name>/
├── manifest.json      # required — metadata (name, id, version, default_zone, icon, min_width, preview, optional config)
├── widget.html        # required — the HTML fragment injected into the bar
├── widget.js          # optional — IIFE that runs once on mount
└── widget.css         # optional — styles injected once per session
```

- `manifest.json` fields: `name`, `id`, `version`, `description`, `default_zone`
  (`left|center|right`), `icon` (a [Phosphor duotone](https://phosphoricons.com) icon name), `min_width`,
  `preview` (static HTML fragment shown in the Widget Manager card only — never rendered live),
  and optionally `config` (user-configurable settings — see the widget development contract in
  `AGENTS.md` §9.4a).
- `widget.js` uses `window.__zenith_invoke` (set by the bar) to call Tauri commands — never
  imports from `@tauri-apps/api` directly.
- Add a widget by creating its folder; remove by deleting it. No code changes outside the
  folder are needed.

---

## Configuration

- **Location:** `%APPDATA%\zenith\config.json` (i.e. `C:\Users\<user>\AppData\Roaming\zenith\`).
- **Format:** JSON. Unknown keys are tolerated (forward-compatible). Missing keys fall back to
  defaults. A corrupt file never crashes the app — it falls back to defaults.

Most settings are best edited in the **Settings** window (right-click the bar → **Settings**),
which writes the config file for you. Power users can edit the JSON directly.

### Key config structure

| Path | Values | Default | Description |
|---|---|---|---|
| `appearance.background.mode` | `"acrylic"`, `"mica"`, `"gradient"`, `"solid"`, `"none"` | `"gradient"` | Window background style |
| `appearance.background.color_top` | hex color | `"#1f2541"` | Top gradient color |
| `appearance.background.color_bottom` | hex color | `"#1a1a1a"` | Bottom gradient color |
| `appearance.background.alpha_top` | `0`–`100` | `60` | Top color opacity |
| `appearance.background.alpha_bottom` | `0`–`100` | `0` | Bottom color opacity |
| `appearance.tint_alpha` | `0`–`255` | `61` | Acrylic accent tint alpha |
| `appearance.bar_height` | `28`–`72` (px) | `40` | Bar height |
| `appearance.margin_*` / `padding_*` | px | `0` / `8` (sides) | Outer margins and inner padding |
| `appearance.corner_radius_*` | px | `0` | Per-corner bar rounding |
| `appearance.theme` | `"dark"`, `"light"`, `"auto"` | `"dark"` | Color theme |
| `monitors` | `"all"` or list of display IDs | `"all"` | Which monitors to show the bar on |
| `updates.auto_update` | `true`, `false` | `true` | Check for updates every 24h |
| `updates.start_with_windows` | `true`, `false` | `true` | Launch on login |
| `storage.onedrive_sync_enabled` | `true`, `false` | `false` | Mirror config to OneDrive |
| `motion.backend` | `"auto"`, `"gpu"`, `"cpu"` | `"auto"` | Animation backend preference |
| `motion.reduced_motion` | `true`, `false` | `false` | Reduce animations |
| `css.custom_enabled` | `true`, `false` | `true` | Inject custom.css |

### Custom CSS

Drop a `custom.css` file in `%APPDATA%\zenith\` and enable it under Settings → Appearance →
Custom CSS. It is hot-reloaded — save the file and the bar restyles live.

---

## Architecture

Zenith is a **Tauri 2** application built in Rust with a plain-TypeScript frontend.

```
zenith/
├── src/                  # Frontend (TypeScript + CSS)
│   ├── windows/          # Window shells: bar, settings, manager, dialog, … 
│   ├── shared/           # Shared kernel: IPC, events, types, config client, widgets
│   ├── styles/           # CSS tokens (.zen-* component library)
│   └── domains/          # Frontend domain clients
├── widgets/              # Standalone widget folders (self-contained)
├── src-tauri/
│   ├── src/
│   │   ├── config/       # Domain: configuration (load/save, model, commands)
│   │   ├── window/       # Domain: AppBar, transparency, monitor management
│   │   ├── widgets/      # Domain: widget registry and manifest scanning
│   │   ├── workspace/    # Domain: virtual desktops (winvd), foreground HWND tracking
│   │   └── …             # Other domains: appearance, motion, menu, dialog, gpu
│   └── capabilities/     # Per-window Tauri permissions
```

Each domain in the Rust backend follows hexagonal architecture: pure services with no Tauri
types in signatures, thin command adapters, and cross-domain communication via events.

---

## Contributing

Contributions are welcome! Here's how:

1. **Report bugs** — open a [GitHub Issue](https://github.com/b7s/zenith/issues/new).
2. **Submit changes** — fork the repo, create a feature branch, and open a pull request.
3. **New widgets** — drop a folder in `widgets/` following the layout above; no Rust changes
   needed unless you need custom IPC.

**Before contributing**, read the full development contract in
[`AGENTS.md`](AGENTS.md) — it covers the project's architecture rules, CSS component system,
transparency contract, animation discipline, and code organization conventions.

### Development setup

```bash
git clone https://github.com/b7s/zenith.git
cd zenith
npm install
npm run tauri dev
```

### Code quality

```bash
npm run typecheck   # TypeScript type checking
cargo check         # Rust type checking (run from src-tauri/)
```

---

## License

Zenith is licensed under the [MIT License](./LICENSE).

```
MIT License

Copyright (c) 2026 b7s

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHER DEALINGS IN THE
SOFTWARE.
```

> Last reviewed: 2026-07-25