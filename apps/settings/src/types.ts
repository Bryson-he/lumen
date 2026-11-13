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
  /** A component style for this widget only; absent uses the one all widgets share. */
  componentStyle?: string;
  /** This widget's own parts; absent uses the arrangement its kind comes with. */
  parts?: Part[];
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
  /** Its own component style; "" goes back to the one all widgets share. */
  componentStyle?: string;
  /** Its parts; an empty list goes back to the arrangement its kind comes with. */
  parts?: Part[];
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
/** The smallest a widget can be, in logical pixels (MIN_SIZE in lumen-ipc). */
export const WIDGET_MIN_SIZE = [96, 56] as const;

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

// ----- The status bar (crates/lumen-ipc/src/bar.rs) -----

export interface LauncherApp {
  name: string;
  /** Full path of the program or shortcut to open. */
  path: string;
}

export type BarModule =
  | { type: "lumen" | "clock" | "date" | "cpu" | "memory" | "gpu" | "media" }
  | { type: "launcher"; apps: LauncherApp[] };

export type BarSlot = "left" | "centre" | "right";

export interface BarConfig {
  enabled: boolean;
  position: "top" | "bottom";
  /** Height in logical pixels. */
  height: number;
  /** "main", "all", or "#2" for monitor number 2. */
  monitor: string;
  /** Reserve the bar's strip of the screen, so maximised windows stop at it. */
  reserveSpace: boolean;
  left: BarModule[];
  centre: BarModule[];
  right: BarModule[];
}

/** BAR_HEIGHT_RANGE and MAX_LAUNCHER_APPS in lumen-ipc. */
export const BAR_HEIGHT_RANGE = [28, 56] as const;
export const MAX_LAUNCHER_APPS = 16;
/** The most modules one slot holds. */
export const MAX_SLOT_MODULES = 12;

// ----- Flyouts (crates/lumen-ipc/src/flyout.rs) -----

export type FlyoutStyle = "pill" | "card";
export type FlyoutPosition = "topLeft" | "topCentre" | "topRight" | "bottomLeft" | "bottomCentre" | "bottomRight";

export interface FlyoutConfig {
  /** Show Lumen's volume flyout, and answer the volume keys in place of Windows. */
  volume: boolean;
  style: FlyoutStyle;
  position: FlyoutPosition;
}

// ----- The launcher (crates/lumen-ipc/src/launcher.rs) -----

export type LauncherHotkey = "altSpace" | "ctrlSpace" | "winAltSpace";

export interface LauncherConfig {
  enabled: boolean;
  hotkey: LauncherHotkey;
}

// ----- Component styles (crates/lumen-ipc/src/style.rs) -----

/** A colour of the active theme. A style names roles, never colours. */
export type StyleRole = keyof Roles;
export type StyleBackground = "glass" | "solid" | "gradient" | "none";

export interface ComponentStyle {
  id: string;
  name: string;
  background: StyleBackground;
  backgroundRole: StyleRole;
  gradientRole: StyleRole;
  /** How solid the background is, in percent (not used by glass). */
  opacity: number;
  cornersFollowTheme: boolean;
  radius: number;
  borderFollowsTheme: boolean;
  borderWidth: number;
  borderRole: StyleRole;
  textRole: StyleRole;
  /** Level bars, the caret, "playing" dots and other highlights. */
  accentRole: StyleRole;
  /** Null uses the theme's font. */
  font: string | null;
  /** Percent of the normal text size. */
  textScale: number;
  /** Percent of the normal padding and gaps. */
  spacing: number;
}

/** Which style each kind of piece uses. */
export interface StyleAssignments {
  widgets: string;
  bar: string;
  flyout: string;
  launcher: string;
}

export interface StylesState {
  /** The styles that come with Lumen (read-only). */
  builtin: ComponentStyle[];
  /** The user's own. */
  styles: ComponentStyle[];
  assigned: StyleAssignments;
}

/** The limits in lumen-ipc's style.rs. */
export const STYLE_LIMITS = { maxStyles: 40, radius: 60, border: 4, textScale: [85, 125], spacing: [70, 150], opacity: [20, 100] } as const;
export const STYLE_FONTS = ["Inter", "Space Grotesk"] as const;

// ----- Widget parts (crates/lumen-ipc/src/parts.rs) -----

export type PartKind = "text" | "bar" | "image";
/** The widget's own text / muted / highlight colour, or one of the theme's roles. */
export type PartColor = "text" | "muted" | "highlight" | keyof Roles;
export type PartAlign = "left" | "centre" | "right";
export type PartWeight = "regular" | "medium" | "semiBold";

export interface Part {
  id: string;
  name: string;
  kind: PartKind;
  /** The live value shown ("clock.time"); empty for fixed text. */
  source: string;
  /** Fixed text, used when source is empty. */
  text: string;
  /** Inside the widget, in logical pixels, at the widget's default size. */
  x: number;
  y: number;
  width: number;
  height: number;
  /** Grows and shrinks with the widget's width. */
  stretch: boolean;
  /** Keeps its distance from the widget's bottom edge. */
  fromBottom: boolean;
  /** Null uses the widget's font. */
  font: string | null;
  size: number;
  weight: PartWeight;
  color: PartColor;
  /** 10–100 percent. */
  opacity: number;
  align: PartAlign;
  hidden: boolean;
}

/** What getPartDefaults returns, per kind of widget. */
export type PartDefaults = Record<WidgetKind, { parts: Part[]; sources: { source: string; label: string }[] }>;

export interface FontList {
  /** Shipped with Lumen. */
  bundled: string[];
  /** Font files the user added. */
  imported: string[];
  /** Installed in Windows. */
  installed: string[];
}

export const PART_COLORS: [PartColor, string][] = [
  ["text", "Text"],
  ["muted", "Text, fainter"],
  ["highlight", "Highlight"],
];
/** The text sizes the slider offers (the core accepts 6–200). */
export const PART_LIMITS = { textSize: [8, 96] } as const;
