# Decisions log

Record any change from `docs/PLAN.md` here: date, what changed, why.

| Date | Decision | Why |
| --- | --- | --- |
| 2026-11-13 | Visual direction D (clean modern with pencil accents), light + dark | Chosen by Bryson from four explorations |
| 2026-11-13 | Settings app uses React + TypeScript (not Svelte) | Settled in the plan |
| 2026-11-13 | M0: `apps/settings/src-tauri` is a member of the root Cargo workspace (crate `lumen-settings`) | One `cargo build` and one `target/` for everything; lets the settings backend share `lumen-ipc` types later |
| 2026-11-13 | M0: settings crate is a plain Windows binary (no Tauri mobile `staticlib`/`cdylib` lib target) | Lumen is Windows-only; the mobile targets only add build time |
| 2026-11-13 | M0: `packages/tokens/build.mjs` generates `dist/tokens.css` (light default, dark via `prefers-color-scheme` or `data-theme`); Rust constants not generated yet | The core has nothing to draw until M5; add the Rust output then |
| 2026-11-13 | M0: added tokens `border.subtle` (dark `#2A2C31`) and `widget.glass.textMuted` (dark `#B9B8B2`) | Both colours are used in the approved designs (sidebar edge, status-strip line, muted text on glass widgets) but were missing from the token table |
| 2026-11-13 | M0: split out `danger.border` and `widget.glass.border`; their light values (`#C0392B`, `rgba(30,31,34,0.10)`) and light `widget.glass.textMuted` (`#5F5E58`) are first guesses | The plan gives these for dark only and no light mock-up uses them; review when the light Widgets screen is built (M6) |
| 2026-11-13 | M0: current toolchain is newer than the plan assumed — Rust 1.97 (edition 2024), Node 24, pnpm 12, Vite 8, TypeScript 7, Tauri 2.12; React stays on 18 as planned | Latest stable at install time; all build cleanly together |
| 2026-11-13 | M0: app icon is a placeholder (pencil-blue "L" on dark) | Tauri needs an `.ico` to build on Windows; no logo has been designed yet |
| 2026-11-13 | M1: role mapping onto Material 3 — `primary`/`secondary`/`tertiary`/`surface`/`onSurface`/`outline` are the Material roles of the same name; `neutral` is Material `surfaceContainerHigh` | The plan names the eight roles but not their Material sources; `neutral` in the example theme is a raised surface one step above `surface` |
| 2026-11-13 | M1: `accent` is not a Material role. It is the wallpaper's second colour furthest in hue from the seed (at least 45° away), else the seed's complement; tone 80 on dark, 40 on light. Monochrome uses white/black | The designs show accent as a contrasting pop colour (the sun against the blue harbour), which no Material role gives |
| 2026-11-13 | M1: variants map to Material TonalSpot, Vibrant, Expressive, Monochrome; `contrast` is stored as -1/0/1 (the slider's Reduced/Standard/High) and passed to Material as -1.0/0.0/1.0 | Matches the three-step slider and the `"contrast": 0` in the plan's example |
| 2026-11-13 | M1: generated dark surfaces are darker than the design mock-ups (about `#0E1513` vs `#1C1D21`) | Material's dark surface is tone 6; kept as generated so contrast guarantees hold. Revisit in M4 if it looks too dark behind widgets |
| 2026-11-13 | M1: the image is sampled on a ~128-point grid straight from the decoded pixels instead of being resized with `image` first | Same result for palette purposes, and skips a full-image resize; decode is the only real cost |
| 2026-11-13 | M1: grey or near-monochrome wallpapers seed from their own faint tint, not Material's stock blue fallback | A grey wallpaper producing a blue theme would look unrelated to the image |
| 2026-11-13 | M1: no CLI argument crate (clap); arguments are parsed by hand | Two commands and five flags; keeps `lumen-theme` (a dependency of the always-on core) small |
| 2026-11-13 | M1: extraction (`extract` -> `SourceColors`) and generation (`Theme::generate`) are separate public steps | Lets the core cache each wallpaper's colours so switching variant or mode never re-reads the image |
| 2026-11-13 | M2: fullscreen watching is event-driven with out-of-context WinEvent hooks (`EVENT_SYSTEM_FOREGROUND` everywhere, `EVENT_OBJECT_LOCATIONCHANGE` only on the foreground window's thread) | No polling, so it costs nothing at idle; out-of-context hooks run in Lumen's process, so nothing is injected into games. Scoping the move/resize hook stops mouse movement waking Lumen |
| 2026-11-13 | M2: "fullscreen" = Windows reports D3D exclusive fullscreen, or the foreground window covers its whole monitor and has no title bar (or Windows reports "busy"). Desktop, taskbar and Task View are excluded by window class | The plan's two signals, plus the title-bar test so a maximised window with an auto-hidden taskbar is not treated as a game |
| 2026-11-13 | M2: fullscreen detection never opens the other process (no exe name); it reports window title and class only | Keeps the anti-cheat rule strict; the title is enough for logs and the status strip |
| 2026-11-13 | M2: setting the accent also writes `AutoColorization = 0`; `accent::snapshot()` / `restore()` put back every value touched, deleting ones that did not exist | Otherwise Windows replaces Lumen's accent on the next wallpaper change. Snapshot/restore is what M3's preview/revert will use |
| 2026-11-13 | M2: the accent's light/dark shades (`AccentPalette`) are simple blends toward white and black, not Windows' own algorithm | Windows' shade algorithm is not public; blends look close. Revisit if Start or Settings highlights look off |
| 2026-11-13 | M2: monitors are numbered left to right, then top to bottom; the stable identifier stored in files will be Windows' monitor device path | Numbers are for people, device paths survive reboots and re-plugging |
| 2026-11-13 | M2: hotkeys (`RegisterHotKey`) are not in `lumen-win` yet | Not part of M2's done-check; first needed for edit mode in M6 |
| 2026-11-13 | M2: `lumen-win` reuses `lumen_theme::Color` and `Mode` | One colour and mode type across crates instead of converting between copies |
