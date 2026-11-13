// Shared building blocks. Every colour here comes from a token (CSS variable)
// or from theme data; nothing is hard-coded.

import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useId, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import type { WallpaperEntry } from "./types";

/** The pencil squiggle under the active nav item. */
export function Squiggle() {
  return (
    <svg className="pencil-stroke" width="54" height="8" viewBox="0 0 54 8" aria-hidden="true">
      <path d="M1 5 C 12 1, 24 7, 36 4 S 50 3, 53 5" fill="none" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

/** The hand-drawn arrow that points a tip at something. */
export function Arrow() {
  return (
    <svg className="pencil-stroke" width="34" height="22" viewBox="0 0 34 22" aria-hidden="true">
      <path d="M32 12 C 24 18, 12 16, 4 8 M4 8 L5 15 M4 8 L11 7" fill="none" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

/** A handwritten note. `accent` uses the pencil colour, otherwise muted. */
export function Tip({ children, accent = false, size = 21 }: { children: ReactNode; accent?: boolean; size?: 20 | 21 | 22 }) {
  return <span className={`tip tip-${size}${accent ? " tip-accent" : ""}`}>{children}</span>;
}

type ButtonKind = "primary" | "outline" | "danger";

export function Button({
  kind = "outline",
  size = "main",
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { kind?: ButtonKind; size?: "main" | "header" | "bar" }) {
  return <button type="button" className={`btn btn-${kind} btn-${size} ${className}`} {...rest} />;
}

/** A row of mutually exclusive choices. */
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  look,
}: {
  label: string;
  options: { value: T; label: string; disabled?: boolean }[];
  value: T | null;
  onChange: (value: T) => void;
  /** "track": pills inside one rounded track ("inset": a sunken one). "grid": separate outlined buttons. */
  look: "track" | "inset" | "grid-2" | "grid-3";
}) {
  return (
    <div role="group" aria-label={label} className={`segmented segmented-${look}`}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={option.value === value}
          disabled={option.disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/**
 * A labelled slider. It keeps its own value while being dragged and reports
 * the change shortly after the last movement, so a drag does not flood the core.
 */
export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  format,
  onCommit,
  disabled,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  format: (value: number) => string;
  onCommit: (value: number) => void;
  disabled?: boolean;
}) {
  const id = useId();
  const [local, setLocal] = useState(value);
  const timer = useRef<number>();
  useEffect(() => setLocal(value), [value]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  return (
    <div className="field">
      <label htmlFor={id} className="field-row">
        <span>{label}</span>
        <span className="muted">{format(local)}</span>
      </label>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={local}
        disabled={disabled}
        onChange={(event) => {
          const next = Number(event.target.value);
          setLocal(next);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => onCommit(next), 140);
        }}
      />
    </div>
  );
}

export function Check({ checked, onChange, children, disabled }: { checked: boolean; onChange: (checked: boolean) => void; children: ReactNode; disabled?: boolean }) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
      {children}
    </label>
  );
}

export function SearchIcon() {
  return (
    <svg className="icon-muted" width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="10" cy="10" r="6.5" fill="none" strokeWidth="1.8" />
      <path d="M15 15 L21 21" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

/** A soft picture made from a palette, for when there is no image to show. */
export function paletteBackdrop(colours: string[]): string {
  const [a, b = a, c = b] = colours;
  if (!a) return "none";
  return `linear-gradient(160deg, ${a} 0%, ${b} 55%, ${c} 100%)`;
}

/** A wallpaper's preview image, falling back to its palette if the file is missing. */
export function WallpaperArt({
  wallpaper,
  thumb,
  colours,
  label,
  className = "",
}: {
  wallpaper: WallpaperEntry | null;
  /** A preview image to show when there is no library entry (a preset's thumbnail). */
  thumb?: string | null;
  colours?: string[];
  label?: string;
  className?: string;
}) {
  const [broken, setBroken] = useState(false);
  const file = wallpaper?.thumb ?? thumb ?? null;
  const source = file ? convertFileSrc(file) : null;
  useEffect(() => setBroken(false), [source]);
  const palette = wallpaper?.palette ?? colours ?? [];

  return (
    <div
      className={`art ${className}`}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      style={{ backgroundImage: paletteBackdrop(palette) }}
    >
      {source && !broken && <img src={source} alt="" draggable={false} onError={() => setBroken(true)} />}
    </div>
  );
}

/** Round colour dots for a palette. */
export function Dots({ colours, size }: { colours: string[]; size: 30 | 36 }) {
  return (
    <>
      {colours.map((colour, index) => (
        <span key={`${colour}-${index}`} className={`dot dot-${size}`} aria-hidden="true" style={{ background: colour }} />
      ))}
    </>
  );
}
