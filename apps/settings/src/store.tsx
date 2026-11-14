// App state: what the core says, plus the actions screens can take.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { call, CoreError, onConnection, onCoreEvent } from "./core";
import type { BarConfig, ComponentStyle, FlyoutConfig, FontList, PartDefaults, WeatherConfig, WeatherState, WidgetFileInfo, LauncherConfig, MonitorInfo, StyleAssignments, StylesState, PresetInfo, PreviewState, PreviewThemeParams, Status, Theme, ThemeState, WallpaperEntry, WidgetsState } from "./types";

export type Screen = "home" | "presets" | "wallpapers" | "theme" | "widgets" | "bar" | "plugins" | "gallery" | "settings";

const IMAGE_FILE = /\.(jpe?g|png|webp|bmp)$/i;
const PRESET_FILE = /\.lumen$/i;

interface Lumen {
  screen: Screen;
  go: (screen: Screen) => void;
  /** Which half of Theme & colours is showing: the colours, or the component styles. */
  themeView: "colours" | "styles";
  setThemeView: (view: "colours" | "styles") => void;

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
  /** The desktop widgets: layout, monitors, selection, edit mode. Null until the core reports it. */
  widgets: WidgetsState | null;
  /**
   * Sends a widget request (updateWidget, addWidget, removeWidget, selectWidget,
   * setLayoutOptions, setEditMode). The result arrives as a new `widgets` value.
   */
  widgetRequest: (method: string, params: unknown) => Promise<void>;
  /** The status bar's settings. Null until the core reports them (or if the bar cannot run). */
  bar: BarConfig | null;
  /** Changes the bar. The screen shows the new settings at once; the core confirms them. */
  setBar: (config: BarConfig) => Promise<void>;
  /** The flyout settings. Null until the core reports them. */
  flyouts: FlyoutConfig | null;
  setFlyouts: (config: FlyoutConfig) => Promise<void>;
  /** Shows the volume flyout on the desktop once, as it looks now. */
  showFlyout: () => Promise<void>;
  /** The launcher's settings. Null until the core reports them. */
  launcher: LauncherConfig | null;
  setLauncher: (config: LauncherConfig) => Promise<void>;
  /** Component styles: the built-in ones, the user's own, and what uses which. Null until the core reports them. */
  styles: StylesState | null;
  /** Replaces the user's styles and assignments. The desktop redraws at once. */
  setStyles: (sheet: { styles: ComponentStyle[]; assigned: StyleAssignments }) => Promise<void>;
  /** Sends exportStyle or importStyle. Null if it failed. */
  styleRequest: <T>(method: string, params: unknown) => Promise<T | null>;
  /** The weather settings and the last forecast. Null until the core reports them. */
  weather: WeatherState | null;
  setWeather: (config: WeatherConfig) => Promise<void>;
  /** The .yaml files in the widgets folder, as last read. */
  widgetFiles: WidgetFileInfo[];
  /** Sends any request to the core; null (with the message strip set) if it failed. */
  request: <T>(method: string, params?: unknown) => Promise<T | null>;
  /** The parts each kind of widget comes with. Null until the core reports them. */
  partDefaults: PartDefaults | null;
  /** The font families a widget part can use. Null until asked for. */
  fonts: FontList | null;
  refreshFonts: () => Promise<void>;
  /** Adds font files to Lumen. */
  importFonts: (paths: string[]) => Promise<void>;
  /** Saved looks, most recently used first. */
  presets: PresetInfo[];
  /** Sends a preset request (savePreset, applyPreset, renamePreset, ...). Null if it failed. */
  presetRequest: <T>(method: string, params: unknown) => Promise<T | null>;
  /** Imports .lumen files. */
  importPresets: (paths: string[]) => Promise<void>;

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
  const [themeView, setThemeView] = useState<"colours" | "styles">("colours");
  const [connected, setConnected] = useState(false);
  const [connectionMessage, setConnectionMessage] = useState<string | null>("Connecting to Lumen…");
  const [theme, setTheme] = useState<Theme | null>(null);
  const [preview, setPreview] = useState<PreviewState | null>(null);
  const [previewEndsAt, setPreviewEndsAt] = useState<number | null>(null);
  const [wallpapers, setWallpapers] = useState<WallpaperEntry[]>([]);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [status, setStatus] = useState<Status | null>(null);
  const [widgets, setWidgets] = useState<WidgetsState | null>(null);
  const [bar, setBarState] = useState<BarConfig | null>(null);
  const [flyouts, setFlyoutsState] = useState<FlyoutConfig | null>(null);
  const [launcher, setLauncherState] = useState<LauncherConfig | null>(null);
  const [styles, setStylesState] = useState<StylesState | null>(null);
  const [partDefaults, setPartDefaults] = useState<PartDefaults | null>(null);
  const [widgetFiles, setWidgetFiles] = useState<WidgetFileInfo[]>([]);
  const [weather, setWeatherState] = useState<WeatherState | null>(null);
  const [fonts, setFonts] = useState<FontList | null>(null);
  const [presets, setPresets] = useState<PresetInfo[]>([]);
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
      // Widgets may be unavailable (no graphics device); the Widgets screen says so.
      call<WidgetsState>("getWidgets").then(setWidgets, () => setWidgets(null)),
      attempt(() => call<PresetInfo[]>("listPresets")).then((list) => list && setPresets(list)),
      call<BarConfig>("getBar").then(setBarState, () => setBarState(null)),
      call<FlyoutConfig>("getFlyouts").then(setFlyoutsState, () => setFlyoutsState(null)),
      call<LauncherConfig>("getLauncher").then(setLauncherState, () => setLauncherState(null)),
      call<StylesState>("getStyles").then(setStylesState, () => setStylesState(null)),
      call<PartDefaults>("getPartDefaults").then(setPartDefaults, () => setPartDefaults(null)),
      call<WidgetFileInfo[]>("getWidgetFiles").then(setWidgetFiles, () => setWidgetFiles([])),
      call<WeatherState>("getWeather").then(setWeatherState, () => setWeatherState(null)),
    ]);
  }, [refreshTheme, refreshWallpapers, refreshStatus, attempt]);

  // Connection and events from the core.
  const refreshAllRef = useRef(refreshAll);
  refreshAllRef.current = refreshAll;
  const refreshThemeRef = useRef(refreshTheme);
  refreshThemeRef.current = refreshTheme;
  const refreshWallpapersRef = useRef(refreshWallpapers);
  refreshWallpapersRef.current = refreshWallpapers;
  useEffect(() => {
    const stops = [
      onConnection((up, message) => {
        setConnected(up);
        setConnectionMessage(up ? null : message);
        if (up) void refreshAllRef.current();
      }),
      onCoreEvent((method, params) => {
        if (method === "themeChanged") void refreshThemeRef.current();
        if (method === "widgetsChanged") setWidgets(params as WidgetsState);
        if (method === "barChanged") setBarState(params as BarConfig);
        if (method === "flyoutsChanged") setFlyoutsState(params as FlyoutConfig);
        if (method === "launcherChanged") setLauncherState(params as LauncherConfig);
        if (method === "stylesChanged") setStylesState(params as StylesState);
        if (method === "widgetFilesChanged") setWidgetFiles(params as WidgetFileInfo[]);
        if (method === "weatherChanged") setWeatherState(params as WeatherState);
        if (method === "presetsChanged") {
          setPresets(params as PresetInfo[]);
          // Presets can bring new wallpapers into the library (saving, importing).
          void refreshWallpapersRef.current();
        }
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
  const gameRunning = status?.gameMode.active ?? false;
  useEffect(() => {
    // Not during a game: the app goes quiet too if it was left open behind one.
    if (screen !== "home" || !connected || gameRunning) return;
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") void refreshStatus();
    }, 2000);
    return () => clearInterval(timer);
  }, [screen, connected, gameRunning, refreshStatus]);

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

  /** Runs a preset request; the list itself updates through the presetsChanged event. */
  const presetRequest = useCallback(
    async <T,>(method: string, params: unknown): Promise<T | null> => {
      const result = await attempt(() => call<T>(method, params));
      // Applying a preset changes the theme and the wallpapers too.
      if (result !== null && method === "applyPreset") await refreshTheme();
      return result;
    },
    [attempt, refreshTheme],
  );

  const importPresets = useCallback(
    async (paths: string[]) => {
      for (const path of paths) await presetRequest<PresetInfo>("importPreset", { path });
    },
    [presetRequest],
  );

  // Dropping files anywhere on the window imports them.
  const importRef = useRef(importWallpapers);
  importRef.current = importWallpapers;
  const importPresetsRef = useRef(importPresets);
  importPresetsRef.current = importPresets;
  useEffect(() => {
    const stop = getCurrentWebview().onDragDropEvent((event) => {
      const kind = event.payload.type;
      if (kind === "enter" || kind === "over") setDropping(true);
      else setDropping(false);
      if (event.payload.type === "drop") {
        // .lumen files are presets; anything else is tried as a wallpaper.
        const presetFiles = event.payload.paths.filter((p) => PRESET_FILE.test(p));
        const others = event.payload.paths.filter((p) => !PRESET_FILE.test(p));
        if (presetFiles.length > 0) {
          setScreen("presets");
          void importPresetsRef.current(presetFiles);
        }
        if (others.length > 0) {
          if (presetFiles.length === 0) setScreen("wallpapers");
          void importRef.current(others);
        }
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

  const widgetRequest = useCallback(
    async (method: string, params: unknown) => {
      await attempt(() => call<WidgetsState>(method, params));
    },
    [attempt],
  );

  const setBar = useCallback(
    async (config: BarConfig) => {
      setBarState(config);
      const set = await attempt(() => call<BarConfig>("setBar", config));
      if (set) setBarState(set);
      // It was refused: show what the core still has.
      else await call<BarConfig>("getBar").then(setBarState, () => {});
    },
    [attempt],
  );

  const setFlyouts = useCallback(
    async (config: FlyoutConfig) => {
      setFlyoutsState(config);
      const set = await attempt(() => call<FlyoutConfig>("setFlyouts", config));
      if (set) setFlyoutsState(set);
      else await call<FlyoutConfig>("getFlyouts").then(setFlyoutsState, () => {});
    },
    [attempt],
  );

  const setLauncher = useCallback(
    async (config: LauncherConfig) => {
      setLauncherState(config);
      const set = await attempt(() => call<LauncherConfig>("setLauncher", config));
      if (set) setLauncherState(set);
      else await call<LauncherConfig>("getLauncher").then(setLauncherState, () => {});
    },
    [attempt],
  );

  const setStyles = useCallback(
    async (sheet: { styles: ComponentStyle[]; assigned: StyleAssignments }) => {
      setStylesState((current) => (current ? { ...current, ...sheet } : current));
      const set = await attempt(() => call<StylesState>("setStyles", sheet));
      if (set) setStylesState(set);
      else await call<StylesState>("getStyles").then(setStylesState, () => {});
    },
    [attempt],
  );

  const styleRequest = useCallback(<T,>(method: string, params: unknown) => attempt(() => call<T>(method, params)), [attempt]);

  const request = useCallback(<T,>(method: string, params?: unknown) => attempt(() => call<T>(method, params)), [attempt]);

  const setWeather = useCallback(
    async (config: WeatherConfig) => {
      setWeatherState((current) => (current ? { ...current, config } : current));
      const set = await attempt(() => call<WeatherConfig>("setWeather", config));
      if (!set) await call<WeatherState>("getWeather").then(setWeatherState, () => {});
    },
    [attempt],
  );

  const refreshFonts = useCallback(async () => {
    await call<FontList>("listFonts").then(setFonts, () => {});
  }, []);

  const importFonts = useCallback(
    async (paths: string[]) => {
      for (const path of paths) {
        const list = await attempt(() => call<FontList>("importFont", { path }));
        if (list) setFonts(list);
      }
    },
    [attempt],
  );

  const showFlyout = useCallback(async () => {
    await attempt(() => call<null>("showFlyout"));
  }, [attempt]);

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
      themeView,
      setThemeView,
      connected,
      connectionMessage,
      theme,
      preview: preview?.theme ?? null,
      shown,
      previewEndsAt,
      wallpapers,
      monitors,
      status,
      widgets,
      widgetRequest,
      bar,
      setBar,
      flyouts,
      setFlyouts,
      showFlyout,
      launcher,
      setLauncher,
      styles,
      setStyles,
      styleRequest,
      weather,
      setWeather,
      widgetFiles,
      request,
      partDefaults,
      fonts,
      refreshFonts,
      importFonts,
      presets,
      presetRequest,
      importPresets,
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
    [screen, themeView, connected, connectionMessage, theme, preview, shown, previewEndsAt, wallpapers, monitors, status, widgets, widgetRequest, bar, setBar, flyouts, setFlyouts, showFlyout, launcher, setLauncher, styles, setStyles, styleRequest, weather, setWeather, widgetFiles, request, partDefaults, fonts, refreshFonts, importFonts, presets, presetRequest, importPresets, error, dropping, previewTheme, keep, revert, importWallpapers, updateWallpaper, setWallpaperOnly, wallpaperOf],
  );

  return <Context.Provider value={value}>{children}</Context.Provider>;
}
