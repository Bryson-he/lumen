// App state: what the core says, plus the actions screens can take.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { call, CoreError, onConnection, onCoreEvent } from "./core";
import type { MonitorInfo, PreviewState, PreviewThemeParams, Status, Theme, ThemeState, WallpaperEntry } from "./types";

export type Screen = "home" | "presets" | "wallpapers" | "theme" | "widgets" | "bar" | "plugins" | "gallery" | "settings";

const IMAGE_FILE = /\.(jpe?g|png|webp|bmp)$/i;

interface Lumen {
  screen: Screen;
  go: (screen: Screen) => void;

  connected: boolean;
  /** Why the core is unreachable, when it is. */
  connectionMessage: string | null;

  /** The kept theme. */
  theme: Theme | null;
  /** The theme being tried on the desktop, if any. */
  preview: Theme | null;
  /** What the desktop shows now: the preview if there is one, else the kept theme. */
  shown: Theme | null;
  /** When the preview reverts by itself (ms since epoch). */
  previewEndsAt: number | null;

  wallpapers: WallpaperEntry[];
  monitors: MonitorInfo[];
  status: Status | null;

  /** The last thing that went wrong, for the message strip. */
  error: string | null;
  dismissError: () => void;
  /** True while files are being dragged over the window. */
  dropping: boolean;

  previewTheme: (params: PreviewThemeParams) => Promise<void>;
  keep: () => Promise<void>;
  revert: () => Promise<void>;
  importWallpapers: (paths: string[]) => Promise<WallpaperEntry | null>;
  updateWallpaper: (id: string, changes: { name?: string; favourite?: boolean; tags?: string[] }) => Promise<void>;
  setWallpaperOnly: (file: string, monitor?: string) => Promise<void>;
  /** The library entry a theme was made from, if it is in the library. */
  wallpaperOf: (theme: Theme | null) => WallpaperEntry | null;
}

const Context = createContext<Lumen | null>(null);

export function useLumen(): Lumen {
  const value = useContext(Context);
  if (!value) throw new Error("useLumen must be used inside <LumenProvider>");
  return value;
}

export function LumenProvider({ children }: { children: ReactNode }) {
  const [screen, setScreen] = useState<Screen>("home");
  const [connected, setConnected] = useState(false);
  const [connectionMessage, setConnectionMessage] = useState<string | null>("Connecting to Lumen…");
  const [theme, setTheme] = useState<Theme | null>(null);
  const [preview, setPreview] = useState<PreviewState | null>(null);
  const [previewEndsAt, setPreviewEndsAt] = useState<number | null>(null);
  const [wallpapers, setWallpapers] = useState<WallpaperEntry[]>([]);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dropping, setDropping] = useState(false);

  /** Runs a core call; on failure shows the message instead of throwing. */
  const attempt = useCallback(async <T,>(work: () => Promise<T>): Promise<T | null> => {
    try {
      const result = await work();
      setError(null);
      return result;
    } catch (e) {
      // Code 0 = the core is unreachable; the connection strip already says so.
      if (!(e instanceof CoreError && e.code === 0)) setError(e instanceof Error ? e.message : String(e));
      return null;
    }
  }, []);

  const applyThemeState = useCallback((state: ThemeState) => {
    setTheme(state.theme);
    setPreview(state.preview);
    setPreviewEndsAt(state.preview ? Date.now() + state.preview.secondsLeft * 1000 : null);
  }, []);

  const refreshTheme = useCallback(async () => {
    const state = await attempt(() => call<ThemeState>("getTheme"));
    if (state) applyThemeState(state);
  }, [attempt, applyThemeState]);

  const refreshWallpapers = useCallback(async () => {
    const list = await attempt(() => call<WallpaperEntry[]>("listWallpapers"));
    if (list) setWallpapers(list);
  }, [attempt]);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await call<Status>("getStatus"));
    } catch {
      // The connection strip covers this.
    }
  }, []);

  const refreshAll = useCallback(async () => {
    await Promise.all([
      refreshTheme(),
      refreshWallpapers(),
      refreshStatus(),
      attempt(() => call<MonitorInfo[]>("listMonitors")).then((list) => list && setMonitors(list)),
    ]);
  }, [refreshTheme, refreshWallpapers, refreshStatus, attempt]);

  // Connection and events from the core.
  const refreshAllRef = useRef(refreshAll);
  refreshAllRef.current = refreshAll;
  const refreshThemeRef = useRef(refreshTheme);
  refreshThemeRef.current = refreshTheme;
  useEffect(() => {
    const stops = [
      onConnection((up, message) => {
        setConnected(up);
        setConnectionMessage(up ? null : message);
        if (up) void refreshAllRef.current();
      }),
      onCoreEvent((method, params) => {
        if (method === "themeChanged") void refreshThemeRef.current();
        if (method === "gameModeChanged") {
          const gameMode = params as Status["gameMode"];
          setStatus((s) => (s ? { ...s, gameMode } : s));
        }
      }),
    ];
    // The core may already be connected before these listeners existed.
    void call<Status>("getStatus").then(
      () => {
        setConnected(true);
        setConnectionMessage(null);
        void refreshAllRef.current();
      },
      () => {},
    );
    return () => stops.forEach((stop) => void stop.then((off) => off()));
  }, []);

  // Live memory and CPU readout, only while Home is on screen and visible.
  useEffect(() => {
    if (screen !== "home" || !connected) return;
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") void refreshStatus();
    }, 2000);
    return () => clearInterval(timer);
  }, [screen, connected, refreshStatus]);

  const importWallpapers = useCallback(
    async (paths: string[]) => {
      let last: WallpaperEntry | null = null;
      const images = paths.filter((p) => IMAGE_FILE.test(p));
      if (images.length === 0 && paths.length > 0) setError("Those files are not images Lumen can use (jpg, png, webp, bmp).");
      for (const path of images) {
        const entry = await attempt(() => call<WallpaperEntry>("importWallpaper", { path }));
        if (entry) last = entry;
      }
      if (images.length > 0) await refreshWallpapers();
      return last;
    },
    [attempt, refreshWallpapers],
  );

  // Dropping image files anywhere on the window imports them.
  const importRef = useRef(importWallpapers);
  importRef.current = importWallpapers;
  useEffect(() => {
    const stop = getCurrentWebview().onDragDropEvent((event) => {
      const kind = event.payload.type;
      if (kind === "enter" || kind === "over") setDropping(true);
      else setDropping(false);
      if (event.payload.type === "drop") {
        setScreen("wallpapers");
        void importRef.current(event.payload.paths);
      }
    });
    return () => void stop.then((off) => off());
  }, []);

  const previewTheme = useCallback(
    async (params: PreviewThemeParams) => {
      const state = await attempt(() => call<PreviewState>("previewTheme", params));
      if (state) {
        setPreview(state);
        setPreviewEndsAt(Date.now() + state.secondsLeft * 1000);
      }
    },
    [attempt],
  );

  const keep = useCallback(async () => {
    const kept = await attempt(() => call<Theme>("commitPreview"));
    if (kept) {
      setTheme(kept);
      setPreview(null);
      setPreviewEndsAt(null);
    } else {
      await refreshTheme();
    }
  }, [attempt, refreshTheme]);

  const revert = useCallback(async () => {
    const state = await attempt(() => call<ThemeState>("revertPreview"));
    if (state) applyThemeState(state);
  }, [attempt, applyThemeState]);

  const updateWallpaper = useCallback<Lumen["updateWallpaper"]>(
    async (id, changes) => {
      const entry = await attempt(() => call<WallpaperEntry>("updateWallpaper", { id, ...changes }));
      if (entry) setWallpapers((list) => list.map((w) => (w.id === id ? entry : w)));
    },
    [attempt],
  );

  const setWallpaperOnly = useCallback(
    async (file: string, monitor?: string) => {
      await attempt(() => call<null>("setWallpaper", monitor ? { file, monitor } : { file }));
    },
    [attempt],
  );

  const wallpaperOf = useCallback(
    (t: Theme | null) => {
      if (!t || t.source.type !== "wallpaper") return null;
      const path = t.source.path.replace(/\\/g, "/");
      return wallpapers.find((w) => w.file === path || w.path.replace(/\\/g, "/") === path) ?? null;
    },
    [wallpapers],
  );

  const shown = preview?.theme ?? theme;

  const value = useMemo<Lumen>(
    () => ({
      screen,
      go: setScreen,
      connected,
      connectionMessage,
      theme,
      preview: preview?.theme ?? null,
      shown,
      previewEndsAt,
      wallpapers,
      monitors,
      status,
      error,
      dismissError: () => setError(null),
      dropping,
      previewTheme,
      keep,
      revert,
      importWallpapers,
      updateWallpaper,
      setWallpaperOnly,
      wallpaperOf,
    }),
    [screen, connected, connectionMessage, theme, preview, shown, previewEndsAt, wallpapers, monitors, status, error, dropping, previewTheme, keep, revert, importWallpapers, updateWallpaper, setWallpaperOnly, wallpaperOf],
  );

  return <Context.Provider value={value}>{children}</Context.Provider>;
}
