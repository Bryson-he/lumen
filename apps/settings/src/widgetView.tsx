// Draws a widget in the app the way the desktop draws it: the card from its
// component style, then each part where the part says. Used for the small
// widgets in the Widgets preview and for the designer's canvas. Every colour
// comes from the theme's roles.

import { useEffect, useState, type CSSProperties } from "react";
import { hexToRgb } from "./color";
import { PART_ICONS } from "./types";
import type { ComponentStyle, Part, PartDefaults, PartKind, Placement, Roles, StylesState, Theme, WidgetFileInfo, WidgetStyle } from "./types";

const GLASS_OPACITY = { none: 1, low: 0.9, medium: 0.78, high: 0.64 } as const;
const WEIGHTS = { regular: 400, medium: 500, semiBold: 600 } as const;

/** What a widget draws with, worked out from the theme, its component style and its own choices. */
export interface Look {
  roles: Roles;
  /** CSS background of the card. */
  background: string;
  border: string;
  borderWidth: number;
  radius: number;
  text: string;
  muted: string;
  highlight: string;
  track: string;
  font: string;
  textScale: number;
  /** Whole-widget opacity, 0–1. */
  opacity: number;
}

export function withAlpha(hex: string, alpha: number): string {
  const rgb = hexToRgb(hex);
  return rgb ? `rgb(${rgb[0]} ${rgb[1]} ${rgb[2]} / ${alpha})` : hex;
}

/** The same rules as `Paint::styled` and `Paint::for_widget` in the core. */
export function resolveLook(theme: Theme, style: ComponentStyle, widget?: WidgetStyle): Look {
  const r = theme.roles;
  const solid = widget?.background === "solid";
  const opacity = solid ? 1 : style.opacity / 100;
  const fill = r[style.backgroundRole];
  let background =
    style.background === "glass"
      ? withAlpha(fill, solid ? 1 : GLASS_OPACITY[theme.style.blur])
      : style.background === "solid"
        ? withAlpha(fill, opacity)
        : style.background === "gradient"
          ? `linear-gradient(135deg, ${withAlpha(fill, opacity)}, ${withAlpha(r[style.gradientRole], opacity)})`
          : "transparent";
  let borderWidth = style.borderFollowsTheme ? theme.style.border : style.borderWidth;
  if (widget?.background === "none") {
    background = "transparent";
    borderWidth = 0;
  }
  const text = r[style.textRole];
  return {
    roles: r,
    background,
    border: style.borderRole === "onSurface" ? withAlpha(r.onSurface, 0.14) : r[style.borderRole],
    borderWidth,
    radius: widget?.radius ?? (style.cornersFollowTheme ? theme.style.radius : style.radius),
    text,
    muted: withAlpha(text, 0.72),
    highlight: r[style.accentRole],
    track: withAlpha(r.outline, 0.55),
    font: style.font ?? theme.style.font,
    textScale: style.textScale / 100,
    opacity: (widget?.opacity ?? 100) / 100,
  };
}

/** The look of one widget: its own component style if it has one, else the one all widgets share. */
export function lookFor(placement: Placement, theme: Theme, styles: StylesState): Look {
  const all = [...styles.builtin, ...styles.styles];
  const style = all.find((item) => item.id === placement.componentStyle) ?? all.find((item) => item.id === styles.assigned.widgets) ?? styles.builtin[0];
  return resolveLook(theme, style, placement.style);
}

/** What a widget is made of: its file's parts, else its own, else the ones its kind comes with. */
export function partsOf(
  placement: Placement,
  defaults: PartDefaults,
  files: WidgetFileInfo[],
  /** Parts of widgets that answered their file's options, by widget id (from the widgets state). */
  resolved?: Record<string, Part[]>,
): { parts: Part[]; defaultSize: readonly [number, number]; file: WidgetFileInfo | null; own: boolean } {
  const kind = defaults[placement.kind];
  if (placement.file) {
    const file = files.find((item) => item.file === placement.file && item.isWidget) ?? null;
    return { parts: resolved?.[placement.id] ?? file?.parts ?? [], defaultSize: file?.size ?? kind.size, file, own: true };
  }
  return { parts: placement.parts ?? kind.parts, defaultSize: kind.size, file: null, own: !!placement.parts };
}

/** Every live value any widget can show: a part may use one from another kind (a clock with a CPU line). */
export function allSources(defaults: PartDefaults | null): { source: string; label: string }[] {
  return defaults ? Object.values(defaults).flatMap((kind) => kind.sources) : [];
}

/** Stand-in live values that keep the clock roughly right. */
export function useSampleValues(): Record<string, string> {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 20_000);
    return () => clearInterval(timer);
  }, []);
  return sampleValues(now);
}

/** The colour a part draws in (before its own opacity). */
export function partColor(part: Part, look: Look): string {
  switch (part.color) {
    case "text":
      return look.text;
    case "muted":
      return look.muted;
    case "highlight":
      return look.highlight;
    default:
      return look.roles[part.color];
  }
}

/** Where a part sits in a widget of this size: as `Part::rect_in` in the core. */
export function rectIn(part: Part, size: readonly [number, number], defaultSize: readonly [number, number]) {
  return {
    x: part.x,
    y: part.fromBottom ? part.y + size[1] - defaultSize[1] : part.y,
    width: part.stretch ? Math.max(1, part.width + size[0] - defaultSize[0]) : part.width,
    height: part.height,
  };
}

/** Stand-in live values, so a widget can be drawn without the desktop's data. */
export function sampleValues(now: Date): Record<string, string> {
  const pieces = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).formatToParts(now);
  return {
    "clock.time": pieces
      .filter((piece) => piece.type !== "dayPeriod")
      .map((piece) => piece.value)
      .join("")
      .trim(),
    "clock.suffix": (pieces.find((piece) => piece.type === "dayPeriod")?.value ?? "").toUpperCase(),
    "clock.date": now.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "short" }),
    "stats.cpu": "12%",
    "stats.memory": "41%",
    "stats.gpu": "8%",
    "media.title": "Track title",
    "media.artist": "Artist",
    "media.progress": "64%",
    "media.playing": "yes",
    "media.active": "yes",
    "weather.temp": "18°",
    "weather.condition": "Partly cloudy",
    "weather.icon": "⛅",
    "weather.feels": "17°",
    "weather.high": "21°",
    "weather.low": "12°",
    "weather.wind": "14 km/h",
    "weather.humidity": "62%",
    "weather.place": "Your place",
    "weather.updated": "Updated 10:42",
    ...Object.fromEntries([1, 2, 3].flatMap((day) => [[`weather.day${day}.name`, ["Mon", "Tue", "Wed"][day - 1]], [`weather.day${day}.icon`, ["🌧️", "☁️", "☀️"][day - 1]], [`weather.day${day}.high`, `${20 - day}°`], [`weather.day${day}.low`, `${11 - day}°`]])),
  };
}
const SAMPLE_FRACTIONS: Record<string, number> = { "stats.cpu": 0.12, "stats.memory": 0.41, "stats.gpu": 0.08, "media.progress": 0.64 };

/** A text part's words: its live value, or its text with any `{live.value}` filled in. */
export function partText(part: Part, values: Record<string, string>): string {
  if (part.source) return values[part.source] ?? "";
  return part.text.replace(/\{([^{}]*)\}/g, (whole, name: string) => values[name.trim()] ?? whole);
}

export const PART_KIND_NAMES: Record<PartKind, string> = { text: "Text", bar: "Bar", image: "Picture", shape: "Shape", ring: "Ring", dial: "Dial", blocks: "Blocks", graph: "Graph", icon: "Icon" };

/** The number a live value starts with: "12%" → 12 (as number_in in the core). */
function numberIn(value: string): number | null {
  const match = /^[+-]?[0-9.]+/.exec(value.trim());
  const parsed = match ? Number(match[0]) : NaN;
  return Number.isFinite(parsed) ? parsed : null;
}

/** Whether a condition holds for these values: the same rules as `holds` in the core. */
export function holds(condition: string, values: Record<string, string>): boolean {
  const text = condition.trim();
  if (!text) return true;
  for (const operator of [">=", "<=", "==", "!=", ">", "<"]) {
    const at = text.indexOf(operator);
    if (at < 0) continue;
    const value = numberIn(values[text.slice(0, at).trim()] ?? "");
    const wanted = Number(text.slice(at + operator.length).trim());
    if (value === null || !Number.isFinite(wanted)) return false;
    return operator === ">=" ? value >= wanted : operator === "<=" ? value <= wanted : operator === "==" ? value === wanted : operator === "!=" ? value !== wanted : operator === ">" ? value > wanted : value < wanted;
  }
  const says = (name: string) => !["", "–", "-", "n/a", "no", "false", "0", "0%"].includes((values[name.trim()] ?? "").trim().toLowerCase());
  return text.startsWith("!") ? !says(text.slice(1)) : says(text);
}

/** A stand-in history for graphs: a gentle wave ending at the value's current reading. */
function sampleHistory(end: number): number[] {
  return Array.from({ length: 30 }, (_, index) => Math.min(1, Math.max(0, end + 0.18 * Math.sin(index / 2.6) * (1 - index / 60) + 0.1 * Math.sin(index / 1.1) - (index === 29 ? 0 : 0.02))));
}

/** One part, at 1:1 in logical pixels. */
function PartView({ part, look, values, size, defaultSize }: { part: Part; look: Look; values: Record<string, string>; size: readonly [number, number]; defaultSize: readonly [number, number] }) {
  const rect = rectIn(part, size, defaultSize);
  const box: CSSProperties = { position: "absolute", left: rect.x, top: rect.y, width: rect.width, height: rect.height, opacity: part.opacity / 100 };
  // While its alert holds, a part takes the alert colour.
  const colour = partColor(part.alertWhen.trim() && holds(part.alertWhen, values) ? { ...part, color: part.alertColor } : part, look);
  if (!holds(part.when, values)) return null;
  if (part.kind === "icon") {
    const glyph = PART_ICONS[part.text.trim().toLowerCase()];
    const side = Math.min(rect.width, rect.height);
    return (
      <span className="wv-icon" style={{ ...box, color: colour, fontFamily: glyph ? "'Segoe Fluent Icons', 'Segoe MDL2 Assets'" : "'Segoe UI Emoji', var(--font-body)", fontSize: side * 0.8, lineHeight: `${rect.height}px` }}>
        {glyph ?? partText(part, values)}
      </span>
    );
  }
  if (part.kind === "ring") {
    const fraction = SAMPLE_FRACTIONS[part.source] ?? (numberIn(values[part.source] ?? "") ?? 0) / 100;
    const side = Math.min(rect.width, rect.height);
    const stroke = Math.min(14, Math.max(2, side / 7));
    const radius = (side - stroke) / 2;
    const round = 2 * Math.PI * radius;
    return (
      <svg style={box} viewBox={`0 0 ${rect.width} ${rect.height}`} aria-hidden="true">
        <circle cx={rect.width / 2} cy={rect.height / 2} r={radius} fill="none" stroke={look.track} strokeWidth={stroke} />
        <circle
          cx={rect.width / 2}
          cy={rect.height / 2}
          r={radius}
          fill="none"
          stroke={colour}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={`${round * Math.min(1, Math.max(0, fraction))} ${round}`}
          transform={`rotate(-90 ${rect.width / 2} ${rect.height / 2})`}
        />
      </svg>
    );
  }
  if (part.kind === "dial") {
    const fraction = Math.min(1, Math.max(0, SAMPLE_FRACTIONS[part.source] ?? (numberIn(values[part.source] ?? "") ?? 0) / 100));
    const side = Math.min(rect.width, rect.height);
    const stroke = Math.min(14, Math.max(2, side / 7));
    const radius = (side - stroke) / 2;
    const round = 2 * Math.PI * radius;
    const arc = round * 0.75;
    // Open at the bottom: the arc starts at the lower left.
    const turn = `rotate(135 ${rect.width / 2} ${rect.height / 2})`;
    return (
      <svg style={box} viewBox={`0 0 ${rect.width} ${rect.height}`} aria-hidden="true">
        <circle cx={rect.width / 2} cy={rect.height / 2} r={radius} fill="none" stroke={look.track} strokeWidth={stroke} strokeLinecap="round" strokeDasharray={`${arc} ${round}`} transform={turn} />
        <circle cx={rect.width / 2} cy={rect.height / 2} r={radius} fill="none" stroke={colour} strokeWidth={stroke} strokeLinecap="round" strokeDasharray={`${arc * fraction} ${round}`} transform={turn} />
      </svg>
    );
  }
  if (part.kind === "blocks") {
    const fraction = Math.min(1, Math.max(0, SAMPLE_FRACTIONS[part.source] ?? 0));
    const lit = fraction <= 0 ? 0 : Math.min(10, Math.max(1, Math.round(fraction * 10)));
    const gap = Math.min(4, Math.max(1.5, rect.width / 60));
    return (
      <span style={{ ...box, display: "flex", gap }}>
        {Array.from({ length: 10 }, (_, index) => (
          <span key={index} style={{ flex: 1, borderRadius: 2, background: index < lit ? colour : look.track }} />
        ))}
      </span>
    );
  }
  if (part.kind === "graph") {
    const readings = sampleHistory(SAMPLE_FRACTIONS[part.source] ?? 0.3);
    const step = rect.width / 59;
    const first = rect.width - step * (readings.length - 1);
    const points = readings.map((value, index) => `${first + step * index},${1 + (rect.height - 2) * (1 - value)}`).join(" ");
    return (
      <svg style={box} viewBox={`0 0 ${rect.width} ${rect.height}`} aria-hidden="true">
        <line x1="0" y1={rect.height - 0.5} x2={rect.width} y2={rect.height - 0.5} stroke={look.track} strokeWidth="1" />
        <polyline points={points} fill="none" stroke={colour} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  }
  if (part.kind === "text") {
    return (
      <span
        className="wv-text"
        style={{ ...box, color: colour, fontFamily: `'${part.font ?? look.font}', var(--font-body)`, fontSize: part.size * look.textScale, fontWeight: WEIGHTS[part.weight], textAlign: part.align === "centre" ? "center" : part.align }}
      >
        {partText(part, values)}
      </span>
    );
  }
  if (part.kind === "bar") {
    const fraction = SAMPLE_FRACTIONS[part.source] ?? 0;
    return (
      <span style={{ ...box, borderRadius: rect.height / 2, background: look.track, overflow: "hidden" }}>
        <span style={{ display: "block", height: "100%", width: `max(${rect.height}px, ${fraction * 100}%)`, borderRadius: rect.height / 2, background: colour }} />
      </span>
    );
  }
  const radius = Math.min(part.radius, Math.min(rect.width, rect.height) / 2);
  if (part.kind === "image") return <span style={{ ...box, borderRadius: radius, background: `linear-gradient(135deg, ${look.highlight}, ${look.roles.tertiary})` }} />;
  return <span style={{ ...box, borderRadius: radius, background: colour }} />;
}

/** A whole widget at 1:1 in logical pixels; scale it with a CSS transform. */
export function WidgetView({ parts, look, size, defaultSize, values, zoom = 1 }: { parts: Part[]; look: Look; size: readonly [number, number]; defaultSize: readonly [number, number]; values: Record<string, string>; zoom?: number }) {
  const radius = Math.min(look.radius, Math.min(size[0], size[1]) / 2);
  // The contents are laid out as designed in a widget `zoom` times smaller, then drawn `zoom` times larger, as on the desktop.
  const laidOut = [size[0] / zoom, size[1] / zoom] as const;
  return (
    <div className="wv" style={{ width: size[0], height: size[1], borderRadius: radius, background: look.background, opacity: look.opacity }}>
      {look.borderWidth > 0 && <span className="wv-border" style={{ borderRadius: radius, border: `${look.borderWidth}px solid ${look.border}` }} />}
      <div style={{ position: "absolute", left: 0, top: 0, width: laidOut[0], height: laidOut[1], transform: zoom === 1 ? undefined : `scale(${zoom})`, transformOrigin: "0 0" }}>
        {parts.map((part) => !part.hidden && <PartView key={part.id} part={part} look={look} values={values} size={laidOut} defaultSize={defaultSize} />)}
      </div>
    </div>
  );
}

/** A thumbnail of a look: a small card with a line of text and a level bar, on a bit of "desktop". */
export function LookSwatch({ look }: { look: Look }) {
  return (
    <span className="look-swatch" aria-hidden="true" style={{ background: `linear-gradient(140deg, ${look.roles.neutral}, ${look.roles.surface} 55%, ${look.roles.primary} 260%)` }}>
      <span className="look-swatch-card" style={{ background: look.background, borderRadius: Math.min(look.radius, 11) * 0.6, border: look.borderWidth > 0 ? `${Math.min(look.borderWidth, 2)}px solid ${look.border}` : "none" }}>
        <span className="look-swatch-text" style={{ background: look.text }} />
        <span className="look-swatch-bar" style={{ background: look.track }}>
          <span style={{ background: look.highlight }} />
        </span>
      </span>
    </span>
  );
}

/** Choices shown as small pictures with a word under each, instead of a drop-down of names. */
export function ChoiceTiles<T extends string>({ label, value, options, disabled, onChange }: { label: string; value: T; options: { value: T; label: string; look: Look }[]; disabled?: boolean; onChange: (value: T) => void }) {
  return (
    <div role="group" aria-label={label} className="choice-tiles">
      {options.map((option) => (
        <button key={option.value} type="button" className="choice-tile" aria-pressed={option.value === value} disabled={disabled} onClick={() => onChange(option.value)}>
          <LookSwatch look={option.look} />
          <span>{option.label}</span>
        </button>
      ))}
    </div>
  );
}
