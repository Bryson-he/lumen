# Lumen — Windows Customisation App Plan

Exported from the living plan doc on 13 Nov 2026: https://claude.ai/artifact/MrtRKGkjqHysBWp5TgKY8R
Design canvas: https://claude.ai/artifact/FyPxse9HxhhQYqfqXS6xub (approved designs are copied into `docs/design/`)

## Vision and goals

One app (working name **Lumen**) that themes the whole Windows desktop from a single source of truth: a wallpaper or a colour you pick. Widgets, bars, menus, flyouts and the app itself all follow that theme, and the whole look saves as a one-click preset.

**Core pillars**

- **One theme, everywhere.** A single token set (colours, fonts, radius, blur, spacing) drives every surface, including third-party widgets.
- **Wallpaper-first matching.** Drop in a wallpaper and get a full light and dark palette generated from it in under a second.
- **Build your own.** Widgets and plugins are small packages with a clear SDK, a live preview and a gallery to share them.
- **Invisible when you're busy.** Near-zero cost at idle and fully paused while a game or fullscreen app runs.

**Resource budget (targets to hold the build to)**

| Scenario | CPU | RAM |
| --- | --- | --- |
| Idle desktop, core + 5 widgets | under 0.5% | under 80 MB |
| Each extra native widget | under 0.1% | under 5 MB |
| Fullscreen game or app detected | 0% (renderers suspended) | widget memory trimmed |
| Settings app open | under 2% | under 150 MB, freed on close |

The settings app and the desktop runtime are separate processes, so closing the editor gives all its memory back.

## References and what to borrow

No current tool does all of this in one place: each one owns a single surface, and getting them to match means hand-copying colours between apps.

| Tool | What it does well | Borrow | Avoid |
| --- | --- | --- | --- |
| Rainmeter | Plain-text, scriptable desktop skins with per-monitor placement | Skin-as-folder packaging, creative freedom, Lua scripting | INI syntax hostile to beginners; bad skins/plugins can leak RAM |
| Wallpaper Engine / Lively | Animated wallpapers, Workshop sharing | Gallery and one-click install; auto-pause on fullscreen | Running a video wallpaper when a static frame would do |
| Windhawk | Mod marketplace for taskbar, Start menu, Explorer; source of every mod readable | Reviewable, source-visible plugins; one-click enable and settings per mod | Process injection can break after Windows updates and may upset game anti-cheat |
| Seelen UI | Full desktop environment: dock, toolbar, tiling, widgets, themed flyouts, dynamic accent | Themed replacement flyouts; one theme engine across surfaces | WebView per surface adds memory; restrictive licence on recent releases |
| YASB / Zebar | Status bars configured in YAML, styled with CSS | CSS-style theming; left/centre/right widget slots | Config files as the only editor |
| matugen / pywal / Material You | Full palette from an image, written into app templates | image → source colour → tonal palettes → roles pipeline; template export | Linux-only templates; no live preview |

**Takeaways**

1. Treat the palette as the product: every surface reads from one token file.
2. Ship a visual editor first and a code view second.
3. Use public Windows APIs and Lumen's own overlay windows, not DLL injection, to stay safe with games and updates.

## Feature set

Five modules share one theme engine; each can be switched off.

**1. Theme engine (the core)**

- A theme is a small JSON token file: colour roles, fonts, corner radius, blur/acrylic strength, border, shadow, animation speed, icon pack.
- Colour roles follow Material 3: primary, secondary, tertiary, neutral, plus surface, on-surface, outline and accent.
- Three generation modes: from wallpaper, from one picked colour, or fully manual.
- Scheme variants: Tonal, Vibrant, Expressive, Monochrome; contrast slider; light/dark/auto (follow Windows, sunset schedule, or per-wallpaper).
- Optional: write the accent into Windows (accent colour, light/dark mode, title-bar colour) so native apps match.

**2. Wallpaper library and presets**

- Library with folders, tags, search; static images, slideshows, lightweight animated wallpapers (video or shader).
- Each wallpaper stores its extracted palette, so switching is instant.
- A **Preset** = wallpaper + theme + widget layout + bar layout + icon pack. Save, duplicate, export as one `.lumen` file, share.
- Per-monitor wallpapers with a shared or per-screen palette.
- Schedules and triggers: time of day, playlist, when a game launches, on battery.

**3. Widgets (Rainmeter-style)**

- Built-ins: clock, date, calendar, weather, CPU/GPU/RAM/disk/network, media player, now-playing art, notes, launcher grid, battery, audio device switcher.
- Drag on the desktop, snap to grid and to each other, lock layout, per-monitor placement.
- Every widget reads theme tokens, so a new wallpaper recolours all of them.
- Click-through and "show only on hover" modes; hide on fullscreen.

**4. Bars, menus and shell surfaces**

- Status bar or dock (top, bottom, side) with left/centre/right slots, as a separate window; the real taskbar can stay or auto-hide.
- Themed replacement flyouts: volume, brightness, media, Wi-Fi, battery, power menu.
- A themed launcher (Start-style search); optional Start-menu replacement later.
- Themed right-click menu on Lumen surfaces first; deeper Explorer theming only as an opt-in advanced plugin.
- Cursor packs and SVG icon packs recoloured from the palette.

**5. Plugins and creator tools**

- Two tiers: **Declarative widgets** (layout file + optional script, safe by default) and **Native plugins** (signed, for system hooks, opt-in with a warning).
- In-app editor: canvas with live preview, property panel bound to tokens, code view, hot reload.
- Data providers as reusable plugins (Spotify, Home Assistant, Steam, calendar, RSS).
- Gallery: browse, install, rate, report; each package lists permissions and measured RAM/CPU cost.

## Architecture and performance

```
              Settings & editor app                 Online gallery
     (Tauri UI, own process, freed on close)   (themes, widgets, presets, plugins)
                     |  theme & layout edits              |  installs packages
                     v                                    v
 +---------------------------------------------------------------------------+
 |  Lumen core service (Rust, always running, under 80 MB)                   |
 |  [Theme engine]   [Preset store]        [Plugin host]       [Perf governor]|
 |  palette from     wallpapers, palettes  Luau sandbox;       pause on       |
 |  image -> tokens  layouts, schedules    native = own proc   fullscreen     |
 +---------------------------------------------------------------------------+
                     |  tokens and live data
                     v
 +---------------------------------------------------------------------------+
 |  Rendered surfaces (DirectComposition, one shared GPU device)             |
 |  [Widgets]       [Bar and dock]      [Flyouts and menus]  [Wallpaper player]|
 +---------------------------------------------------------------------------+
                     |
   Windows: wallpaper and accent APIs, WinEvent hooks, media session, DWM blur
```

| Layer | Choice | Why |
| --- | --- | --- |
| Core service | Rust with the `windows` crate | Low memory, no GC pauses, direct Win32/DirectX access |
| Rendering | Direct2D + DirectWrite on DirectComposition, one shared GPU device | One renderer for all surfaces instead of a browser per widget |
| Widget format | Declarative layout file + Luau scripts | Easy for beginners, sandboxed, small per-script VM |
| Web widgets (optional tier) | One shared WebView2 instance, on demand | Web devs can build widgets without a browser per widget |
| Settings app | Tauri 2 (Rust backend shared with core) + React + TypeScript | Polished custom UI; closes fully |
| Native plugins | Signed DLLs in a separate host process, named pipes to core | A crashing plugin can't take down the desktop |
| Config | JSON files in `%APPDATA%\Lumen` | Readable, diffable, easy to back up |

**How Lumen stays light**

- **Event-driven, not polling.** Widgets redraw only when data changes; a clock redraws once a minute unless it shows seconds.
- **Shared data providers.** One CPU sampler feeds every CPU widget; one media-session listener feeds every player.
- **Fullscreen/game detection.** Suspend rendering and animated wallpapers, then trim working memory.
- **Per-package budgets.** Governor measures each widget's CPU/RAM, warns past a limit, can auto-suspend.
- **Battery mode.** Static wallpaper, 30 fps cap, slower refresh when unplugged.
- **No injection by default.** Shell look comes from Lumen's own windows and public APIs.

**Example widget file**

```
widget "Now Playing" {
  size: 320 x 96
  data: media.session
  Card(bg: surface.container, radius: theme.radius, blur: theme.blur) {
    Image(src: data.art, size: 72, radius: 8)
    Stack {
      Text(data.title, style: title, color: on.surface)
      Text(data.artist, style: body, color: on.surface.variant)
      Progress(value: data.position / data.length, color: primary)
    }
  }
}
```

Every colour is a theme role, never a hex value, so widgets recolour whenever the wallpaper changes.

## Base app UI/UX

The app should feel like a calm gallery, not a control panel: big previews, few words, every change shown live on the real desktop before it is kept. A left sidebar holds the sections; Home leads with the active preset so switching the look takes one click.

**Key screens**

| Screen | Purpose | Main interactions |
| --- | --- | --- |
| Home | Active preset and quick switching | Apply, shuffle, edit, recent presets |
| Presets | Saved looks | Save current, duplicate, schedule, export `.lumen`, share |
| Wallpapers | Library with extracted palettes | Drag-drop import, tags, per-monitor assign, "make theme from this" |
| Theme & colours | Palette and style tokens | Source colour, scheme variant, contrast, radius, blur, fonts, light/dark |
| Widgets | Place and configure widgets | Desktop edit mode, grid snapping, inspector, lock layout |
| Bar, dock & menus | Shell surfaces | Slot editor, flyout styles, launcher, context menus |
| Plugins & editor | Build widgets and plugins | Canvas + code split view, token picker, hot reload, publish |
| Gallery | Community content | Browse, install, rate; permissions and measured cost |

**Interaction principles**

1. **Live preview, safe undo.** Changes apply to the real desktop instantly; a bar offers Keep or Revert, auto-reverting after 15 s.
2. **Desktop edit mode.** Win+Alt+E dims the desktop; widgets show handles and snap guides; inspector slides in; Esc leaves.
3. **Three-step onboarding.** Pick a wallpaper, pick a vibe (Minimal, Gamer, Productivity, Cozy), bar on/off.
4. **Command palette (Ctrl+K).** Settings, presets, wallpapers and widgets from one box.
5. **Tray menu.** Quick preset switching, pause all, edit mode.
6. **Progressive depth.** Basics first, "Advanced" expander and "View as code" toggle.
7. **Accessible by default.** Contrast check on every generated palette, full keyboard navigation, respects text scaling and reduced motion.

## Visual design: Direction D (chosen)

A clean modern app on a faint dot grid, with hand-drawn pencil touches (squiggle underlines, arrows, handwritten tips) as the only sketch elements. Light and dark modes. Approved mock-ups: `docs/design/`.

**Colour tokens**

| Token | Light | Dark | Used for |
| --- | --- | --- | --- |
| `bg` | `#FAFAF7` | `#121315` | Window background |
| `bg.dots` | `rgba(30,31,34,0.13)` | `rgba(236,236,234,0.09)` | 1 px dots on a 22 px grid over `bg` |
| `sidebar` | `#FFFFFF` | `#18191C` | Left navigation |
| `surface` | `#FFFFFF` | `#1C1D21` | Cards, panels |
| `surface.raised` | `#F2F1EC` | `#25272C` | Selected nav item, inputs, status bar |
| `surface.selected` | `#F2F1EC` | `#2C2E34` | Selected segment or chip |
| `border` | `#E3E1DA` | `#2E3036` | Card/panel outlines, dividers |
| `border.strong` | `#D6D3C9` | `#3A3D44` | Inputs, outline buttons |
| `text` | `#1E1F22` | `#ECECEA` | Headings and body |
| `text.muted` | `#5F5E58` | `#A3A29C` | Labels, hints |
| `button.primary` | bg `#1E1F22`, text `#FFFFFF` | bg `#ECECEA`, text `#121315` | Main action per screen |
| `pencil` (accent) | `#2F5BEA` | `#8EA6FF` | Squiggles, arrows, tips, selection outlines, sliders, checkboxes |
| `success` | `#2E7D4F` | `#4ADE80` | Status dot, applied |
| `danger` | `#C0392B` | `#FF8A7A` (border `#4A3533`) | Remove, errors |
| `widget.glass` | `rgba(255,255,255,0.78)` | `rgba(28,29,33,0.78)`, border `rgba(236,236,234,0.14)` | Desktop widgets (Glass) |

The pencil accent follows the active preset's primary colour by default; users can pick a fixed one. Designed alternatives: red `#C0392B`, green `#2E7D4F`, violet `#7A4FD6` (light); coral `#FF8A7A`, mint `#6EE7A8`, lilac `#C4A7FF` (dark).

**Type** (bundle the font files locally; all are open-licence Google Fonts)

| Role | Font | Size / weight |
| --- | --- | --- |
| Page title (H1) | Space Grotesk | 32 / 600 |
| Card title | Space Grotesk | 30 hero, 22 panels / 600 |
| Section heading | Space Grotesk | 18 / 600 |
| Body and controls | Inter | 15 / 400; labels 13–14 / 500 |
| Logo and handwritten tips | Caveat | logo 38 / 700; tips 20–22 / 600 |
| Hex values | system monospace | 12 |

**Pencil accent rules**

- Only for: squiggle under the active nav item, one or two handwritten tips per screen, hand-drawn arrows, selection outlines and snap guides in edit mode, slider/checkbox accents.
- Never for body text or long labels; tips can be switched off in Settings ("Show sketch notes").
- Squiggle: SVG 54×8, path `M1 5 C 12 1, 24 7, 36 4 S 50 3, 53 5`, stroke 2, round caps.
- Arrow: SVG 34×22, path `M32 12 C 24 18, 12 16, 4 8 M4 8 L5 15 M4 8 L11 7`, stroke 1.8, round caps.

**Shape and spacing**

- Corner radius: 8 nav items, inputs, small buttons · 10 buttons and search · 12 tiles and previews · 14 panels · 16 hero card · 999 filter pills.
- Main content padding 30 px top, 40 px sides; gaps 18–28 px; sidebar 220 px wide, 18 px padding.
- Controls at least 40 px tall, main buttons 44–46 px.
- 1 px borders; no drop shadows on cards; motion 150–250 ms ease-out, theme crossfade 300 ms, off with Windows reduce-motion.

## Screen specs

Four screens are designed and approved; the other five follow the same shell and patterns. Build every screen in light and dark.

**Shared shell**

- Left sidebar: Lumen logo (Caveat), then Home, Presets, Wallpapers, Theme & colours, Widgets, Bar dock & menus, Plugins & editor, Gallery; Settings pinned at the bottom in muted text. Active item: `surface.raised`, weight 600, pencil squiggle under the label.
- Page header: H1 left with optional handwritten tip under it; page actions right.
- Search box (Ctrl+K) opens the command palette.
- Responsive: sidebar and content in a wrapping row; narrow windows stack them.

**Home** (designed: `home-light.dc.html`, `home-dark.dc.html`)

- Hero card: live wallpaper preview, "Active preset" label, preset name, five palette dots plus tip "from your wallpaper" with a pencil arrow, buttons Edit preset (primary) and Shuffle.
- Quick switch: three preset cards (thumbnail + name); one click applies.
- Status strip: green dot, live memory and CPU readout, game-mode state.

**Theme & colours** (designed: `theme-dark.dc.html`)

- Source switch: From wallpaper · Pick a colour · Manual.
- Left: source wallpaper card with Change link; scheme variant buttons (Tonal, Vibrant, Expressive, Monochrome); Contrast slider (Reduced, Standard, High); Mode (Light, Dark, Auto).
- Centre: eight role swatches (Primary, Secondary, Tertiary, Neutral, Surface, On surface, Outline, Accent) with hex; click to fine-tune. Below: live preview of a status bar and two widgets in the current palette.
- Right: Style panel — Corner radius, Blur/glass, Border, Font, Animation speed, "Also set the Windows accent colour".
- Bottom bar: "Previewing on your desktop", Revert and Keep changes; auto-revert after 15 s.

**Wallpapers** (designed: `wallpapers-dark.dc.html`)

- Header: search ("Search or filter by colour") and + Import (drag-and-drop too).
- Filter pills: All, Static, Animated, Favourites, + Tag.
- Tile grid (thumbnail, name, Static/Animated); selected tile has a 2 px pencil border.
- Side panel: preview, name, detail line (animated ones pause in games), palette dots, Show on (Monitor 1, Monitor 2, Both), Make theme from this, Set as wallpaper only.

**Widgets** (designed: `widgets-dark.dc.html`)

- Header: Snap to grid, Lock layout, Edit on desktop (Win+Alt+E), + Add widget.
- Desktop preview: wallpaper with a 24 px grid; widgets as on the desktop. Selected widget: pencil outline, four corner handles, size badge, dashed snap guide.
- Widget library row: Clock, Now playing, System stats, Weather, Calendar, Notes, App launcher, Browse gallery.
- Inspector (Layout, Style, Data tabs): width, height, X, Y; background (Glass, Solid surface, None); opacity; data source; Corners follow theme, Only show on hover, Hide when a game is fullscreen; Remove widget.

**Still to design (first-pass specs — build in the same style)**

| Screen | Contents |
| --- | --- |
| Presets | Grid of saved presets (thumbnail, name, last used); Save current look; per-preset menu: rename, duplicate, schedule, export `.lumen`, delete; Import |
| Bar, dock & menus | Bar on/off, position, height, auto-hide; slot editor with Left/Centre/Right drop zones; flyout style picker; launcher hotkey; tray-menu preview |
| Plugins & editor | Installed plugins with permissions and live CPU/RAM cost; enable/disable; "New widget" opens split editor (canvas + code) with token picker and hot reload |
| Gallery | Search, categories (Themes, Presets, Widgets, Plugins), cards with preview, author, rating, permissions, measured cost; Install |
| Settings | Start with Windows, game-mode detection, battery mode, show sketch notes, app light/dark/auto, hotkeys, backup and restore, about and updates |

## Roadmap and risks

Phases: **1 Foundation** (theme engine, tokens, wallpaper palettes, settings app shell) → gate: idle under 80 MB → **2 Widgets MVP** (built-in widgets, desktop edit mode, presets and export) → gate: used daily → **3 Shell surfaces** (status bar and dock, themed flyouts, launcher, tray menu) → gate: no game slowdowns → **4 Creators** (widget SDK and editor, Luau and web widgets, gallery and sharing).

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Windows updates break shell tweaks | Bars or menus stop working | Overlay windows and public APIs; risky hooks only in optional plugins with version checks |
| Game anti-cheat flags the app | Bans or games refuse to launch | No DLL injection in the core; game-mode auto-pause; warnings on any injecting plugin |
| Community plugins leak memory/CPU | "Heavy" reputation | Per-package budgets, sandboxed scripts, cost shown in gallery, auto-suspend |
| Malicious gallery uploads | Security incident | Declarative widgets get no file/network access unless granted; native plugins signed and reviewed |
| Scope creep | Never ships | Hold to the phase gates |

## Build handoff

**1. PC setup (Windows 11)**

1. Visual Studio 2022 Build Tools with "Desktop development with C++" and the latest Windows 11 SDK.
2. Rust via `rustup` (stable, `x86_64-pc-windows-msvc`).
3. Node.js LTS and pnpm (`npm i -g pnpm`).
4. Tauri 2 CLI: `cargo install tauri-cli --version "^2"`.
5. Git; WebView2 is already on Windows 11.

Check these are installed as part of M0; install or tell Bryson what's missing.

**2. Repo layout**

```
Lumen/
  Cargo.toml                 # Rust workspace
  crates/
    lumen-core/              # always-on service: state, preview/commit, governor, tray, IPC server
    lumen-theme/             # wallpaper -> palette -> roles -> tokens (+ CLI)
    lumen-win/               # Windows API wrappers (wallpaper, accent, dark mode, fullscreen, hotkeys)
    lumen-render/            # Direct2D + DirectComposition windows for widgets and the bar
    lumen-widgets/           # widget file parser, Luau runtime, built-in widgets
    lumen-ipc/               # shared message types (serde) for core <-> settings app
  apps/
    settings/                # Tauri 2 + React + TypeScript + Vite
  packages/
    tokens/                  # tokens.json -> CSS variables (app) + Rust constants (core)
  docs/
    PLAN.md, DECISIONS.md, design/
```

**3. Libraries**

| Need | Choice |
| --- | --- |
| Windows APIs | `windows` crate (windows-rs) |
| Palette generation | `material-colors` crate (Rust port of Material Color Utilities); downscale the image to ~128 px first with `image` |
| Widget scripting | `mlua` with the `luau` feature (sandboxed) |
| CPU, RAM, GPU stats | `sysinfo` (CPU/RAM); DXGI adapter queries for GPU |
| Async and IPC | `tokio`; named pipe `\\.\pipe\lumen-core` carrying JSON-RPC 2.0 |
| Files and config | `serde`, `serde_json`, `notify`, `zip` |
| Logging | `tracing` + `tracing-subscriber` |
| Settings UI | React 18 + TypeScript + Vite; CSS variables from `packages/tokens`; Inter, Space Grotesk, Caveat bundled locally |

**4. Windows integration points**

| Feature | How |
| --- | --- |
| Wallpaper per monitor | `IDesktopWallpaper` COM interface (`SetWallpaper(monitorId, path)`) |
| Light/dark mode | `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` (`AppsUseLightTheme`, `SystemUsesLightTheme`), then broadcast `WM_SETTINGCHANGE` with `ImmersiveColorSet` |
| Windows accent colour | No public API: registry under `HKCU\Software\Microsoft\Windows\DWM` and `...\Explorer\Accent`; optional, re-test after Windows updates |
| Fullscreen/game detection | `SHQueryUserNotificationState` (busy / D3D fullscreen) + foreground-window-covers-monitor check |
| Desktop widgets | Borderless tool windows (`WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`) at the bottom of the z-order; DirectComposition for transparency |
| Status bar | App bar via `SHAppBarMessage(ABM_NEW)` |
| Glass/Mica | `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` (Win 11 22H2+); Tauri window effects for the settings app |
| Now playing | `GlobalSystemMediaTransportControlsSessionManager` (WinRT) |
| Hotkeys | `RegisterHotKey` (Win+Alt+E edit mode); Ctrl+K inside the app |
| Start with Windows | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` entry for `lumen-core` |

**5. File formats**

User data lives in `%APPDATA%\Lumen\` (`themes/`, `presets/`, `wallpapers/`, `widgets/`, `config.json`).

```json
// themes/midnight-harbour.json
{
  "name": "Midnight Harbour",
  "source": { "type": "wallpaper", "path": "wallpapers/midnight-harbour.jpg" },
  "variant": "tonal", "mode": "dark", "contrast": 0,
  "roles": { "primary": "#8EA6FF", "secondary": "#B7C3E8", "tertiary": "#D9A7C7",
             "neutral": "#2A2D35", "surface": "#1C1D21", "onSurface": "#ECECEA",
             "outline": "#4A4E58", "accent": "#F2CC8F" },
  "style": { "radius": 12, "blur": "medium", "border": 1, "font": "Inter", "motion": "normal" },
  "setWindowsAccent": true
}
```

- **Preset** (`.lumen`): a zip with `preset.json` (name, theme file, wallpaper per monitor, widget layout, bar config), the theme JSON and the wallpaper image(s).
- **Widget package**: a folder with `lumen-widget.json` (id, name, version, author, default size, data sources, permissions), a layout file (format above) and optional `script.luau`.

**6. Build order** (tick each when its done-check passes)

- [x] **M0 · Skeleton.** Check the PC setup, create the workspace and Tauri app. Done when `cargo build` passes and `cargo tauri dev` opens an empty window.
  - Done 2026-11-13. pnpm and the Tauri CLI were the only missing tools. `packages/tokens` generates CSS variables now; Rust constants are deferred until the core draws something (M5). Four extra tokens added from the designs. Details in `docs/DECISIONS.md`; window capture in `docs/milestones/m0-empty-window.png`.
- [x] **M1 · Palette engine.** `lumen-theme extract <image> --variant tonal --mode dark` prints the theme JSON above. Done when all four variants and both modes work, unit tests pass, and a 4K image takes under 300 ms.
  - Done 2026-11-13. 28 unit tests pass. Release build: 4K JPEG about 25–30 ms, 4K PNG about 39 ms, 6400×4000 JPEG about 65 ms. Also added `lumen-theme from-color <#RRGGBB>`. How the eight roles map onto Material 3, and how `accent` is chosen, is in `docs/DECISIONS.md`; swatch sheet in `docs/milestones/m1-palettes.png`.
- [x] **M2 · Windows integration.** Read/set wallpaper per monitor, switch light/dark, set accent, log fullscreen on/off. Done when each works from a small test command.
  - Done 2026-11-13. Test command is `lumen-win` (`monitors`, `wallpaper`, `mode`, `accent`, `fullscreen`, with `--revert-after`). Checked on the two-monitor desktop; every change was reverted. Fullscreen was checked with a borderless test window only: exclusive-fullscreen and real games are still to test (M8). Hotkeys are left for M6. Details in `docs/DECISIONS.md`.
- [x] **M3 · Core service.** `lumen-core` runs from the tray, stores state in `%APPDATA%\Lumen`, serves IPC methods (`getTheme`, `previewTheme`, `commitPreview`, `revertPreview`, `listWallpapers`, `importWallpaper`, `applyPreset`; events `themeChanged`, `gameModeChanged`). Done when a preview auto-reverts after 15 s.
  - Done 2026-11-13. On the real desktop a preview (wallpaper on both monitors, light mode, accent) reverted by itself at 15.0 s; 24 unit tests. Idle: about 18 MB working set, about 0.03% CPU. Test client is `lumen-ctl`. `applyPreset` reads a plain `presets/<id>.json` for now; the `.lumen` zip comes in M7. The tray menu is the basic one (Open Lumen, Quit) and was not click-tested. Known gap: a preview is not undone if the core is killed mid-preview. Details in `docs/DECISIONS.md`.
- [ ] **M4 · Settings app screens.** Shell, Home, Theme & colours, Wallpapers built to the specs and tokens, light and dark, wired to the core. Done when picking a wallpaper and a variant changes the real desktop and Keep/Revert works.
- [ ] **M5 · Renderer and first widgets.** Clock, System stats, Now playing on the desktop, recolouring on `themeChanged`. Done when idle use is under 0.5% CPU and 80 MB RAM with all three running.
- [ ] **M6 · Widgets screen and edit mode.** Drag, grid snap, inspector, saved layout, Win+Alt+E on the real desktop. Done when the layout survives a restart.
- [ ] **M7 · Presets.** Save, apply, export, import `.lumen`; Quick switch on Home; tray menu. Done when export + re-import on a clean profile reproduces the look.
- [ ] **M8 · Game mode and governor.** Suspend rendering in fullscreen games; per-widget CPU/RAM. Done when a fullscreen game shows 0% Lumen CPU.

After M8: bar and dock, themed flyouts, launcher, then the widget SDK, editor and gallery. Test on more than one monitor and with games that use anti-cheat.
