// Draws a widget in the app the way the desktop draws it: the card from its
// component style, then each part where the part says. Used for the small
// widgets in the Widgets preview and for the designer's canvas. Every colour
// comes from the theme's roles.

import { useEffect, useState, type CSSProperties } from "react";
import { hexToRgb } from "./color";
import type { ComponentStyle, Part, PartDefaults, PartKind, Placement, Roles, StylesState, Theme, WidgetStyle } from "./types";

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
  };
}
const SAMPLE_FRACTIONS: Record<string, number> = { "stats.cpu": 0.12, "stats.memory": 0.41, "stats.gpu": 0.08, "media.progress": 0.64 };

/** A text part's words: its live value, or its text with any `{live.value}` filled in. */
export function partText(part: Part, values: Record<string, string>): string {
  if (part.source) return values[part.source] ?? "";
  return part.text.replace(/\{([^{}]*)\}/g, (whole, name: string) => values[name.trim()] ?? whole);
}

export const PART_KIND_NAMES: Record<PartKind, string> = { text: "Text", bar: "Bar", image: "Picture", shape: "Shape" };

/** One part, at 1:1 in logical pixels. */
function PartView({ part, look, values, size, defaultSize }: { part: Part; look: Look; values: Record<string, string>; size: readonly [number, number]; defaultSize: readonly [number, number] }) {
  const rect = rectIn(part, size, defaultSize);
  const box: CSSProperties = { position: "absolute", left: rect.x, top: rect.y, width: rect.width, height: rect.height, opacity: part.opacity / 100 };
  const colour = partColor(part, look);
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
export function WidgetView({ parts, look, size, defaultSize, values }: { parts: Part[]; look: Look; size: readonly [number, number]; defaultSize: readonly [number, number]; values: Record<string, string> }) {
  const radius = Math.min(look.radius, Math.min(size[0], size[1]) / 2);
  return (
    <div className="wv" style={{ width: size[0], height: size[1], borderRadius: radius, background: look.background, opacity: look.opacity }}>
      {look.borderWidth > 0 && <span className="wv-border" style={{ borderRadius: radius, border: `${look.borderWidth}px solid ${look.border}` }} />}
      {parts.map((part) => !part.hidden && <PartView key={part.id} part={part} look={look} values={values} size={size} defaultSize={defaultSize} />)}
    </div>
  );
}
