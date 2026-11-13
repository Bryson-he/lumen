# Lumen — notes for Claude Code

Lumen is a Windows 11 customisation app: one theme (generated from a wallpaper) drives widgets, a status bar, flyouts and the app itself, and whole looks save as presets.

**The full plan is `docs/PLAN.md`. Read it before doing anything.** Approved screen designs are in `docs/design/` (see its README).

## How to work

- Build milestone by milestone (M0 → M8 in `docs/PLAN.md` → "Build handoff" → "Build order").
- Finish one milestone, run its done-check, then stop and report to Bryson before starting the next.
- Tick the milestone off in `docs/PLAN.md` when its done-check passes, and add a short line under it with anything that changed from the plan.
- If something in the plan turns out not to work (a crate API differs, a Windows API behaves differently), pick the closest working option, note why in `docs/DECISIONS.md`, and carry on.

## Non-negotiables

- Stay light: idle target under 0.5% CPU and 80 MB RAM for the core plus widgets; nothing renders while a fullscreen game runs.
- No DLL injection or hooking into other processes in the core (game anti-cheat safety). Use public Windows APIs and Lumen's own windows.
- Every colour in the app and widgets comes from theme tokens, never hard-coded hex in components.
- The settings app is a separate process (Tauri 2 + React + TypeScript) and must free its memory when closed.
- UI must match `docs/design/` and the tokens, fonts and spacing in `docs/PLAN.md` → "Visual design", in both light and dark.

## Stack

Rust workspace (`crates/`), Tauri 2 + React 18 + TypeScript + Vite settings app (`apps/settings`), pnpm for JS. Windows APIs via the `windows` crate. Palette via `material-colors`. See `docs/PLAN.md` for the full library list and repo layout.

## Machine

Windows 11 desktop gaming PC. Support more than one monitor from the start. Test game mode with games that use anti-cheat, and never ship anything that touches other processes.
