# Lumen — Windows Customisation App Plan

Exported from the living plan doc on 13 Nov 2026: https://claude.ai/artifact/MrtRKGkjqHysBWp5TgKY8R
Design canvas: https://claude.ai/artifact/FyPxse9HxhhQYqfqXS6xub (approved designs are copied into `docs/design/`)

## Vision and goals

One app (working name **Lumen**) that themes the whole Windows desktop from a single source of truth: a wallpaper or a colour you pick. Widgets, bars, menus, flyouts and the app itself all follow that theme, and the whole look saves as a one-click preset.

**Core pillars**

- **One theme, everywhere.** A single token set (colours, fonts, radius, blur, spacing) drives every surface, including third-party widgets.
- **Wallpaper-first matching.** Drop in a wallpaper and get a full light and dark palette generated from it in under a second.
- **Build your own.** Widgets and plugins are small packages with a clear SDK, a live preview and a gallery to share them. The built-in pieces (bar, widgets, flyouts, launcher) can be restyled too, with styles that keep following the theme (M12).
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
| Widget format | A list of parts, as a YAML file (M15), + Luau scripts later | Readable and hand-editable like the tools people know; the designer edits the same file |
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

**Example widget file** (Superseded 2026-11-14: widget files will be YAML; see `docs/CUSTOMISATION.md` and M15.)

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
- [x] **M4 · Settings app screens.** Shell, Home, Theme & colours, Wallpapers built to the specs and tokens, light and dark, wired to the core. Done when picking a wallpaper and a variant changes the real desktop and Keep/Revert works.
  - Done 2026-11-13. Checked in the real app on the two-monitor desktop: wallpaper + variant changed the desktop, Revert and the 15 s countdown restored it, Keep persisted it. Release app is about 88 MB (Task Manager's measure) and frees everything on close. The core gained `getStatus`, `listMonitors`, `setWallpaper`, `updateWallpaper` and wallpaper thumbnails. Quick switch shows recent wallpapers until presets exist (M7); the other five screens are placeholders. Not tested by hand: the file picker and drag-and-drop import. Details in `docs/DECISIONS.md`; screenshots in `docs/milestones/m4-*.png`.
- [x] **M5 · Renderer and first widgets.** Clock, System stats, Now playing on the desktop, recolouring on `themeChanged`. Done when idle use is under 0.5% CPU and 80 MB RAM with all three running.
  - Done 2026-11-13. Release core with all three widgets, 66 s at idle: about 0.006% of the whole CPU and 62.7 MB working set. Widgets recolour live on preview, keep and revert, and hide while a fullscreen app is in front. The three widgets are drawn natively; the widget file format and Luau wait for the creator tools. No real backdrop blur yet (the blur setting changes glass opacity). Widgets are click-through until edit mode (M6). Details in `docs/DECISIONS.md`; picture in `docs/milestones/m5-widgets.png`.
- [x] **M6 · Widgets screen and edit mode.** Drag, grid snap, inspector, saved layout, Win+Alt+E on the real desktop. Done when the layout survives a restart.
  - Done 2026-11-13. A layout with moved, resized and restyled widgets on both monitors came back identical after closing and restarting Lumen. Edit mode on the real desktop: dim layer, drag to move with grid and edge snapping and dashed guides, corner resize, Esc to finish. The inspector lives in the settings app, not on the desktop. The Widgets screen adds a zoom control and monitor switcher for large monitors; its widget pictures show sample data. Details in `docs/DECISIONS.md`; pictures in `docs/milestones/m6-*.png`.
- [x] **M7 · Presets.** Save, apply, export, import `.lumen`; Quick switch on Home; tray menu. Done when export + re-import on a clean profile reproduces the look.
  - Done 2026-11-13. A preset exported from one profile and imported into an empty one reproduced the look exactly: theme, both monitors' wallpapers (same file hash), mode, accent and widget layout. Presets name monitors by number, not hardware id, so they carry between PCs. Applying is immediate (no Keep/Revert). The tray menu has preset switching, Edit widgets and Pause widgets; it was exercised by sending its commands directly, not by clicking. Schedules are not built. Details in `docs/DECISIONS.md`.
- [x] **M8 · Game mode and governor.** Suspend rendering in fullscreen games; per-widget CPU/RAM. Done when a fullscreen game shows 0% Lumen CPU.
  - Done 2026-11-13. With a borderless fullscreen window in front for 26 s, the release core used 0.0 ms of CPU and its working set fell from 62.7 MB to 1.3 MB; the widget windows and graphics device are released, not just hidden, and come back when it closes. The governor measures each widget (CPU share, memory, redraws) and switches off one that stays far over budget. **Not yet tested with a real game:** exclusive-fullscreen titles and games with anti-cheat still need a run on this PC. Details in `docs/DECISIONS.md`.

- [x] **M9 · Status bar and dock.** A themed bar at the top or bottom of a monitor, as Lumen's own window, with left / centre / right slots of modules (Lumen, clock, date, CPU, memory, GPU, now playing, app launcher). It reserves its strip of the screen so maximised windows sit beside it, recolours with the theme, is part of presets, and is released in game mode like the widgets. The "Bar, dock & menus" screen turns it on and edits position, height, monitor and slots. Off by default. Done when the bar shows on the real desktop with a maximised window resting against it, recolours on `themeChanged`, its settings survive a restart, and idle cost stays inside the budget.
  - Scope note (written when starting M9): side (vertical) bars, auto-hide, and auto-hiding the Windows taskbar are left for later; so are the themed flyouts and launcher, which are their own milestones.
  - Done 2026-11-14. On the real desktop (one 3840×2160 monitor at 150%): the bar reserved its strip (work area 0,54 to 3840,2160) and the maximised windows moved to rest against it, at the top and at the bottom; it recoloured on a theme preview and on revert; its settings survived a restart of the core; it and the widgets were released during a fullscreen window and came back; turning it off or quitting gave the space back. Idle over 60 s with the bar and three widgets (release): 0.015% CPU, 22.6 MB. Changes from the entry above are in `docs/DECISIONS.md`.

- [x] **M10 · Themed volume flyout.** Lumen's own volume flyout in the theme's colours: it appears when the volume keys are pressed or the volume or mute changes, shows the level (and, in the "card" style, what is playing), and fades away by itself. While it is on, Lumen answers the volume keys itself so the Windows flyout does not also appear. Two styles and six screen positions, chosen on the "Bar, dock & menus" screen; part of presets; off by default; never shown in game mode (the keys go back to Windows then). Done when pressing a volume key changes the volume by one step and shows Lumen's flyout on the real desktop, it recolours with the theme, its settings survive a restart, and nothing is left running between appearances.
  - Scope note (written when starting M10): this PC is a desktop, so brightness, Wi-Fi and battery flyouts are left out; the power menu and a clickable volume slider are left for later.
  - Done 2026-11-14. On the real desktop: a volume-down key press moved the volume one step (100% to 98%) and showed Lumen's flyout, and volume-up put it back; mute showed "Muted"; the flyout recoloured on a theme preview; both styles and two positions were placed correctly; nothing appeared with a fullscreen window in front; the settings survived a restart of the core; and its window was gone a few seconds after each appearance. The settings panel was driven in light and dark. Changes and what was not tested are in `docs/DECISIONS.md`.

- [x] **M11 · Launcher.** A themed search-and-launch panel: a hotkey opens it in the middle of the screen, typing filters the installed apps (classic and Store apps alike), arrows and Enter or a click open one, Esc or clicking elsewhere closes it. It learns which apps are opened most. The "Bar, dock & menus" screen turns it on and picks the hotkey; part of presets; off by default; the hotkey is released in game mode. Done when the hotkey opens the panel on the real desktop, typing finds an app and Enter starts it, it recolours with the theme, its settings survive a restart, and nothing is left running once it is closed.
  - Scope note (written when starting M11): apps only. Files, settings pages, web search, a calculator and replacing the Start menu are left for later.
  - Done 2026-11-14. On the real desktop: the hotkey opened the panel on the monitor the mouse was on and it took the keyboard; typing "note" listed Notepad, Sticky Notes and Git Release Notes with their real icons; the arrow keys moved the selection; Enter on "notepad" started Notepad and the launcher listed it first next time; Esc, the hotkey again and a fullscreen window each closed it, and focus went back to the window used before; it recoloured on a theme preview; its settings survived a restart of the core; no launcher window was left once closed. The settings panel was driven in light and dark. Changes and what was not tested are in `docs/DECISIONS.md`.

- [x] **M12 · Component styles (make each piece look your own).** Asked for by Bryson on 2026-11-14. Every built-in piece (widget cards, the bar and its modules, flyouts, the launcher) is drawn from a named *component style* instead of fixed numbers in code. A style is a small file of properties: shape and corner radius, padding and spacing, background (glass, solid, gradient, none) and its opacity, border, shadow, text sizes and weights, and which theme *role* each colour comes from (for example "level bar: primary", "label: on-surface, 65%"). Colours are always roles, never fixed hex, so a custom style still follows whatever theme is active. Lumen ships a few styles per piece; the settings app gets a style editor (property panel bound to tokens, live preview on the desktop, "duplicate and edit", per-piece overrides such as "this widget only"); styles are saved in presets and exported inside `.lumen` files and on their own. Done when a style made in the editor changes the real bar, a widget, the flyout and the launcher, stays correct across two different themes in light and dark, and survives export and import.
  - Rule from now on: new surfaces take their colours and shape from `Paint` (the one place the theme becomes drawing values), so M12 can swap in styles without redrawing each surface.
  - Beyond restyling: completely new pieces (your own widget or bar module with its own layout) come with the widget SDK and editor, which use the same roles and style properties.
  - Done 2026-11-14. A style made in the editor (gradient from surface to accent, 26 px corners, a 2 px primary border, tertiary highlights, Space Grotesk at 110%, spacing 130%) changed the real bar, all three widgets, the volume flyout and the launcher at once; under a different light theme the same style took that theme's colours; it survived a restart of the core and an export to a `.lumenstyle` file and import back; one widget was given its own style; deleting the style put everything back to the theme default. The editor was driven in light and dark. What differs from the entry above (no shadow, spacing and per-piece overrides) and what was not tested are in `docs/DECISIONS.md`.

### Next stage (proposed 2026-11-14 from Bryson's first hands-on test; order and scope to be confirmed before starting)

Feedback that shaped it: resizing by the corners in the app did not work; it was not clear how a style is saved; there was no obvious "no background" look; editing the look was confusing overall. Wanted next: styling that reaches *everything*, including a different font for each piece of text and where each piece sits; styling by drag and drop; and weather from an online service as a first real data source.

- [x] **M13 · Widget parts (style every piece of a widget).** Each built-in widget stops being one fixed drawing and becomes a list of named *parts* (the clock: time, AM/PM, date; system stats: title, three labels, three values, three bars; now playing: cover, title, artist, progress). Every part has its own position and size inside the widget, font, text size and weight, colour role, alignment, and can be hidden. Lumen ships each widget's default arrangement; changes are saved per widget and travel in presets. More fonts: any font installed on the PC, plus font files imported into Lumen and carried inside `.lumen` exports. Component styles (M12) keep setting the card; parts set what is on it. This is also the widget file format the plan describes, so it is the first half of the widget SDK. Done when the clock's time, date and AM/PM each have a different font, size and colour role on the real desktop, one of them is moved and one hidden, the widget still recolours with the theme, and the arrangement survives a restart and a preset round trip.
  - Done 2026-11-14. On the real desktop the clock's time was set to Georgia at 46 px in the accent colour and centred, its date to Consolas in the primary colour and moved below the time, and AM/PM hidden, all from the new Parts tab; under a different light theme the parts took that theme's colours; the arrangement survived a restart of the core and a preset save, reset and apply; a font file added to Lumen was listed and loaded. With no parts of their own the three widgets look as before. Changes from the entry above (fonts are not yet carried inside `.lumen` exports; the cover no longer grows with the widget's height) and what was not tested are in `docs/DECISIONS.md`.
- [x] **M14 · Widget designer (drag and drop).** A designer in the settings app for one widget at a time: the widget drawn large on a canvas as it really looks (its component style and its parts); click a part to select it, drag it to move, drag its corners to resize, with snapping and guides; buttons to add parts (text, a live value, a level bar, a shape, a divider), duplicate, remove and change which is in front; the Parts panel edits the selected part; undo and redo; "reset". The real widget on the desktop updates as you drag. Text parts become templates: `CPU {stats.cpu}` mixes fixed text and live values. The small widgets in the Widgets preview are drawn by the same renderer, so they finally show the real arrangement and style. Done when a widget is rearranged and restyled entirely with the mouse, a new text part with a live value in it and a shape are added, the result matches on the desktop, and undo brings back the previous arrangement.
  - Scope note (written when starting M14): editing the bar's modules on the same canvas is left out, because the bar's modules are not parts yet; pictures other than the now-playing cover wait for M15's widget files, which have somewhere to keep them.
  - Done 2026-11-14. In the designer the clock's date was dragged to a new place and its time resized by a corner; a text part reading `CPU {stats.cpu}`, a shape and a CPU level bar were added from the Add menu; arrow keys nudged the bar and undo and redo stepped back and forward. The real clock on the desktop matched the canvas after each change and now redraws every two seconds for its CPU parts. The small widgets in the Widgets preview are drawn by the same renderer from their real parts and style. Driven with pointer and key events sent to the page, in light and dark. What was not tested is in `docs/DECISIONS.md`.
- [x] **M15 · Widget files (YAML) and code view.** From the review in `docs/CUSTOMISATION.md`. Every widget can be a readable **YAML file** in the data folder (the format sketched in that document): its size, parts, and three things borrowed from Rainmeter: *variables* (name a value once, use it in many parts), *shared text styles* (one "caption" look many parts use), and *includes* (one file of variables and styles a whole set of widgets shares). Lumen watches the files and redraws the moment one is saved; a mistake shows a plain message with the line number and keeps the last good version on screen. The designer reads and writes the same file, and a code view in the app shows it beside the canvas. Widgets made this way appear in the widget library, can be duplicated, and export and import as a folder package. Colours stay theme roles: a file that names a hex colour is refused with an explanation. Done when a widget file edited in Notepad changes the desktop on save, a variable changed in an included file restyles two widgets at once, a broken file reports its line and leaves the old look up, and a change made in the designer appears in the file.
  - Done 2026-11-14. Two widget files written by hand into the widgets folder, sharing one included file of variables and styles, appeared in the library and on the desktop; changing a variable in the shared file restyled both at once (font and size) within a second of saving; a hex colour was refused with the part and line named while both widgets kept their last good look; dragging a part in the designer rewrote the file with its `$variables`, `style:` and `include:` kept; the code view saved an edit, explained a typed mistake with its line, and cleared it when fixed. Changes from the entry above (no folder packages yet) and what was not tested are in `docs/DECISIONS.md`.
- [x] **M16 · Weather and online data.** A weather widget and a weather bar module, as the first data that comes from the internet. Source: Open-Meteo (free, no account or API key) for the forecast and for finding a place by name; another provider can be added behind the same interface later. Data sources gain settings of their own (weather: the place and the units), written in the widget file and editable in the designer. Shows current temperature and conditions, today's high and low, and a few days ahead, as ordinary parts. Lumen only goes online once weather is switched on and a place is chosen, says so plainly, refreshes about every 20 minutes, keeps the last reading when offline, and makes no requests while a game is in front. Done when the widget shows the real weather for a chosen place, updates on its own, survives being offline, and idle cost stays inside the budget.
  - Done 2026-11-14. With weather off nothing was fetched and the widget said so. A place search found London and Paris; choosing Paris put the real forecast on the desktop (5°, clear, high and low, and three days ahead with pictures) and on the bar. Open-Meteo's forecast server was dropping about four requests in five from this network during the check, which is how the retry rule came to be (10 s, then doubling to 5 minutes). With a fullscreen window in front a refresh fetched nothing. Restarted and pointed at a host that does not exist, Lumen showed the kept forecast at once and reported the failure without losing it. Idle over 60 s with the weather widget and four others (release): 0.004% CPU, 22.1 MB. What differs from the entry above and what was not tested are in `docs/DECISIONS.md`.
- [ ] **M17 · Behaviour and more kinds of part.** Also from the review. Parts can *do* things: a click opens an app, a file or a web page, or plays and pauses; hover can reveal a part. Parts can *react*: `when:` conditions show or hide a part, and thresholds change its colour role (a bar turns from primary to accent above 90%). New kinds of part: ring (a circular gauge), graph (a value over the last minutes), and icon. Done when a widget made in the designer has a clickable part, a bar that changes colour past a threshold, a ring and a graph, all still inside the governor's budget.
- [ ] **M18 · Easier to find your way (usability pass).** From Bryson's first test: a short guided "make your look" flow on Home (wallpaper, colours, style, widgets, save as a preset); plain wording for style and background choices with small pictures of each; one place that explains what is saved automatically and what waits for Keep. Done when someone new can go from a fresh install to a saved preset without help.

After these: the scripted SDK (Luau), widget options an author exposes for users to adjust, packages and the gallery, the plugin screen, and writing the palette out to other apps (terminal, editor).
