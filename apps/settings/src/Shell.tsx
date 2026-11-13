// The shared shell: sidebar, page area, the Keep/Revert bar and the command palette.

import { open } from "@tauri-apps/plugin-dialog";
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useLumen, type Screen } from "./store";
import { Button, SearchIcon, Squiggle } from "./ui";

export const NAV: { screen: Screen; label: string }[] = [
  { screen: "home", label: "Home" },
  { screen: "presets", label: "Presets" },
  { screen: "wallpapers", label: "Wallpapers" },
  { screen: "theme", label: "Theme & colours" },
  { screen: "widgets", label: "Widgets" },
  { screen: "bar", label: "Bar, dock & menus" },
  { screen: "plugins", label: "Plugins & editor" },
  { screen: "gallery", label: "Gallery" },
];

/** Opens the file picker and imports the chosen images. */
export function useImportDialog() {
  const { importWallpapers } = useLumen();
  return async () => {
    const picked = await open({
      multiple: true,
      title: "Import wallpapers",
      filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png", "webp", "bmp"] }],
    });
    if (!picked) return null;
    return importWallpapers(Array.isArray(picked) ? picked : [picked]);
  };
}

export function Shell({ children }: { children: ReactNode }) {
  const { screen, go, preview, connected, connectionMessage, error, dismissError, dropping } = useLumen();
  const [paletteOpen, setPaletteOpen] = useState(false);

  // Ctrl+K opens the command palette from anywhere.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((isOpen) => !isOpen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app">
      <nav className="sidebar" aria-label="Main">
        <div className="logo">Lumen</div>
        {NAV.map((item) => (
          <NavLink key={item.screen} active={screen === item.screen} onClick={() => go(item.screen)}>
            {item.label}
          </NavLink>
        ))}
        <NavLink active={screen === "settings"} onClick={() => go("settings")} pinned>
          Settings
        </NavLink>
      </nav>

      <main className={`main main-${screen}`}>
        {!connected && connectionMessage && (
          <div className="strip strip-danger" role="status">
            <span className="status-dot status-dot-danger" />
            {connectionMessage}
          </div>
        )}
        {error && (
          <div className="strip strip-danger" role="alert">
            <span>{error}</span>
            <button type="button" className="link" onClick={dismissError}>
              Dismiss
            </button>
          </div>
        )}
        <OpenPalette.Provider value={() => setPaletteOpen(true)}>{children}</OpenPalette.Provider>
        {preview && <PreviewBar />}
      </main>

      {dropping && (
        <div className="drop-overlay" aria-hidden="true">
          <span className="tip tip-22 tip-accent">drop to add to your wallpapers</span>
        </div>
      )}
      {paletteOpen && <CommandPalette onClose={() => setPaletteOpen(false)} />}
    </div>
  );
}

function NavLink({ active, pinned, onClick, children }: { active: boolean; pinned?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" className={`nav-link${active ? " nav-active" : ""}${pinned ? " nav-pinned" : ""}`} aria-current={active ? "page" : undefined} onClick={onClick}>
      {children}
      {active && <Squiggle />}
    </button>
  );
}

const OpenPalette = createContext<() => void>(() => {});

/** The "Search everything · Ctrl K" box in a page header. */
export function SearchBox() {
  const openPalette = useContext(OpenPalette);
  return (
    <button type="button" className="search search-button" onClick={openPalette} aria-label="Search everything (Ctrl K)">
      <SearchIcon />
      <span className="search-placeholder">Search everything</span>
      <kbd>Ctrl K</kbd>
    </button>
  );
}

/** Page header: title and optional handwritten tip on the left, actions on the right. */
export function PageHeader({ title, tip, children }: { title: string; tip?: string; children?: ReactNode }) {
  return (
    <header className="page-header">
      <div className="page-title">
        <h1>{title}</h1>
        {tip && <span className="tip tip-21">{tip}</span>}
      </div>
      {children}
    </header>
  );
}

/** "Previewing on your desktop" with Revert and Keep, counting down to the auto-revert. */
function PreviewBar() {
  const { previewEndsAt, keep, revert } = useLumen();
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(timer);
  }, []);
  const seconds = previewEndsAt ? Math.max(0, Math.ceil((previewEndsAt - now) / 1000)) : null;

  return (
    <div className="preview-bar" role="status">
      <span className="preview-bar-text">
        <span className="status-dot" />
        Previewing on your desktop
        {seconds !== null && <span className="muted"> · reverts in {seconds} s</span>}
      </span>
      <span className="preview-bar-actions">
        <Button size="bar" onClick={() => void revert()}>
          Revert
        </Button>
        <Button size="bar" kind="primary" onClick={() => void keep()}>
          Keep changes
        </Button>
      </span>
    </div>
  );
}

interface Command {
  label: string;
  hint: string;
  run: () => void;
}

function CommandPalette({ onClose }: { onClose: () => void }) {
  const { go, wallpapers, preview, keep, revert, previewTheme } = useLumen();
  const importFromDialog = useImportDialog();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const list = useRef<HTMLUListElement>(null);

  const commands = useMemo<Command[]>(() => {
    const all: Command[] = [];
    if (preview) {
      all.push({ label: "Keep changes", hint: "Preview", run: () => void keep() });
      all.push({ label: "Revert preview", hint: "Preview", run: () => void revert() });
    }
    for (const item of [...NAV, { screen: "settings" as Screen, label: "Settings" }]) {
      all.push({ label: item.label, hint: "Go to", run: () => go(item.screen) });
    }
    all.push({
      label: "Import wallpapers…",
      hint: "Wallpapers",
      run: () => {
        go("wallpapers");
        void importFromDialog();
      },
    });
    for (const wallpaper of wallpapers) {
      all.push({
        label: `Make theme from ${wallpaper.name}`,
        hint: "Wallpaper",
        run: () => {
          go("theme");
          void previewTheme({ source: { type: "wallpaper", path: wallpaper.file } });
        },
      });
    }
    const q = query.trim().toLowerCase();
    return q ? all.filter((c) => c.label.toLowerCase().includes(q) || c.hint.toLowerCase().includes(q)) : all;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, wallpapers, preview]);

  useEffect(() => setIndex(0), [query]);
  useEffect(() => {
    list.current?.children[index]?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const choose = (command: Command | undefined) => {
    if (!command) return;
    onClose();
    command.run();
  };

  return (
    <div className="palette-backdrop" onMouseDown={onClose}>
      <div className="palette" role="dialog" aria-modal="true" aria-label="Search everything" onMouseDown={(event) => event.stopPropagation()}>
        <label className="search palette-search">
          <SearchIcon />
          <input
            autoFocus
            value={query}
            placeholder="Search everything"
            aria-label="Search everything"
            aria-controls="palette-list"
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") onClose();
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setIndex((i) => Math.min(i + 1, commands.length - 1));
              }
              if (event.key === "ArrowUp") {
                event.preventDefault();
                setIndex((i) => Math.max(i - 1, 0));
              }
              if (event.key === "Enter") choose(commands[index]);
            }}
          />
          <kbd>Esc</kbd>
        </label>
        <ul id="palette-list" className="palette-list" role="listbox" ref={list}>
          {commands.map((command, i) => (
            <li key={`${command.hint}-${command.label}`} role="option" aria-selected={i === index}>
              <button type="button" tabIndex={-1} className={i === index ? "palette-item palette-item-active" : "palette-item"} onMouseEnter={() => setIndex(i)} onClick={() => choose(command)}>
                <span>{command.label}</span>
                <span className="muted small">{command.hint}</span>
              </button>
            </li>
          ))}
          {commands.length === 0 && <li className="palette-empty muted">Nothing matches “{query}”.</li>}
        </ul>
      </div>
    </div>
  );
}
