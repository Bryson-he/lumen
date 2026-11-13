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
