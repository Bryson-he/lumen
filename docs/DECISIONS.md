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
