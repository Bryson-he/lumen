// TypeScript mirrors of the Rust types in crates/lumen-theme and crates/lumen-ipc.

export type Mode = "light" | "dark";
export type Variant = "tonal" | "vibrant" | "expressive" | "monochrome";
export type Blur = "none" | "low" | "medium" | "high";
export type Motion = "slow" | "normal" | "fast";

export interface Roles {
  primary: string;
  secondary: string;
  tertiary: string;
  neutral: string;
  surface: string;
  onSurface: string;
  outline: string;
  accent: string;
}

export interface Style {
  radius: number;
  blur: Blur;
  border: number;
  font: string;
  motion: Motion;
}

export type Source = { type: "wallpaper"; path: string } | { type: "color"; value: string } | { type: "manual" };

export interface Theme {
  name: string;
  source: Source;
  variant: Variant;
  mode: Mode;
  /** -1 reduced, 0 standard, 1 high. */
  contrast: number;
  roles: Roles;
  style: Style;
  setWindowsAccent: boolean;
}

export interface PreviewState {
  theme: Theme;
  secondsLeft: number;
}

export interface ThemeState {
  theme: Theme | null;
  preview: PreviewState | null;
}

/** Anything left out is taken from the current theme by the core. */
export interface PreviewThemeParams {
  theme?: Theme;
  source?: Source;
  variant?: Variant;
  mode?: Mode;
  contrast?: number;
  style?: Style;
  setWindowsAccent?: boolean;
  name?: string;
  wallpaperMonitor?: string;
}

export interface WallpaperEntry {
  id: string;
  name: string;
  /** Relative to the data folder, e.g. wallpapers/x.jpg. */
  file: string;
  path: string;
  thumb: string;
  kind: "static" | "animated";
  palette: string[];
  addedAt: number;
  favourite: boolean;
  tags: string[];
}

export interface MonitorInfo {
  number: number;
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  primary: boolean;
  wallpaper: string | null;
}

export interface Status {
  version: string;
  dataDir: string;
  memoryMb: number;
  cpuPercent: number;
  gameMode: { active: boolean; app: string | null };
  previewing: boolean;
}

export const ROLE_NAMES: [keyof Roles, string][] = [
  ["primary", "Primary"],
  ["secondary", "Secondary"],
  ["tertiary", "Tertiary"],
  ["neutral", "Neutral"],
  ["surface", "Surface"],
  ["onSurface", "On surface"],
  ["outline", "Outline"],
  ["accent", "Accent"],
];

// ----- Desktop widgets (crates/lumen-ipc/src/widgets.rs) -----

export type WidgetKind = "clock" | "systemStats" | "nowPlaying";
export type WidgetBackground = "glass" | "solid" | "none";

export interface WidgetStyle {
  background: WidgetBackground;
  /** 20–100 percent. */
  opacity: number;
  /** Corner radius in px; null follows the theme. */
  radius: number | null;
  showOnHover: boolean;
  hideInGames: boolean;
}

export interface Placement {
  id: string;
  kind: WidgetKind;
  /** Monitor id; absent means the primary monitor. */
  monitor?: string;
  x: number;
  y: number;
  width: number;
  height: number;
  style: WidgetStyle;
}

export interface WidgetLayout {
  widgets: Placement[];
  snap: boolean;
  locked: boolean;
}

export interface MonitorArea {
  id: string;
  number: number;
  primary: boolean;
  /** Work area size in logical pixels. */
  width: number;
  height: number;
  wallpaper: string | null;
}

export interface WidgetsState {
  layout: WidgetLayout;
  monitors: MonitorArea[];
  editMode: boolean;
  selected: string | null;
  guides: { x: number | null; y: number | null; monitor: string | null };
  /** Widgets the governor switched off for using too much CPU. */
  suspended: string[];
}

export interface UpdateWidgetParams {
  id: string;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  monitor?: string;
  style?: WidgetStyle;
  /** True while still dragging: snapped and shown, not yet saved. */
  dragging?: boolean;
}

export const WIDGET_NAMES: Record<WidgetKind, string> = {
  clock: "Clock",
  systemStats: "System stats",
  nowPlaying: "Now playing",
};

/** The grid widgets snap to, in logical pixels (GRID in lumen-ipc). */
export const WIDGET_GRID = 24;

// ----- Presets (crates/lumen-ipc/src/presets.rs) -----

export interface PresetInfo {
  id: string;
  name: string;
  /** Absolute path of a small preview image, if it has a wallpaper. */
  thumb: string | null;
  /** Five colours that sum up its theme. */
  palette: string[];
  /** Seconds since 1970. */
  createdAt: number;
  /** 0 if never applied. */
  lastUsedAt: number;
  widgetCount: number;
  /** The preset the current look came from, until the look is changed by hand. */
  active: boolean;
}

// ----- The governor (crates/lumen-ipc/src/widgets.rs) -----

export interface WidgetCost {
  id: string;
  /** Share of the whole CPU, in percent, over the last measuring window. */
  cpuPercent: number;
  memoryMb: number;
  drawsPerMinute: number;
  status: "ok" | "overBudget" | "suspended";
}

/** What one widget may use before the governor steps in. */
export const WIDGET_CPU_BUDGET = 0.1;
export const WIDGET_MEMORY_BUDGET_MB = 5;
