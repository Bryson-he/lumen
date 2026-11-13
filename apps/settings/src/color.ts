// Small colour helpers for search and the "Auto" mode choice.

export function hexToRgb(hex: string): [number, number, number] | null {
  const match = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!match) return null;
  const value = parseInt(match[1], 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

/** Perceived lightness, 0 (black) to 1 (white). */
export function luminance(hex: string): number {
  const rgb = hexToRgb(hex);
  if (!rgb) return 0.5;
  const [r, g, b] = rgb.map((c) => {
    const v = c / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Hue in degrees (0–360) and saturation (0–1). */
function hueSaturation(hex: string): { hue: number; saturation: number; lightness: number } {
  const [r, g, b] = (hexToRgb(hex) ?? [128, 128, 128]).map((c) => c / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const lightness = (max + min) / 2;
  const delta = max - min;
  if (delta === 0) return { hue: 0, saturation: 0, lightness };
  const saturation = delta / (1 - Math.abs(2 * lightness - 1));
  let hue = max === r ? ((g - b) / delta) % 6 : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4;
  hue = (hue * 60 + 360) % 360;
  return { hue, saturation, lightness };
}

/** Colour words people type, as hue ranges in degrees. */
const HUE_WORDS: Record<string, [number, number]> = {
  red: [345, 15],
  orange: [15, 45],
  yellow: [45, 70],
  green: [70, 165],
  teal: [165, 195],
  cyan: [165, 195],
  blue: [195, 255],
  purple: [255, 290],
  violet: [255, 290],
  pink: [290, 345],
};

/**
 * Whether a search query, read as a colour, matches a palette. Returns null
 * when the query is not a colour at all (then it is a name search).
 */
export function colourMatch(query: string, palette: string[]): boolean | null {
  const q = query.trim().toLowerCase();
  const typed = hexToRgb(q);
  if (q.startsWith("#") && typed) {
    return palette.some((hex) => {
      const rgb = hexToRgb(hex);
      return rgb !== null && Math.hypot(rgb[0] - typed[0], rgb[1] - typed[1], rgb[2] - typed[2]) < 70;
    });
  }
  if (q === "grey" || q === "gray" || q === "black" || q === "white") {
    return palette.slice(0, 2).every((hex) => {
      const c = hueSaturation(hex);
      if (c.saturation > 0.18) return false;
      return q === "black" ? c.lightness < 0.25 : q === "white" ? c.lightness > 0.75 : true;
    });
  }
  const range = HUE_WORDS[q];
  if (!range) return null;
  const [from, to] = range;
  return palette.some((hex) => {
    const c = hueSaturation(hex);
    const inRange = from > to ? c.hue >= from || c.hue < to : c.hue >= from && c.hue < to;
    return inRange && c.saturation > 0.15;
  });
}

/** "Auto" mode: dark themes for dark wallpapers, light for light ones. */
export function autoMode(colours: string[]): "light" | "dark" {
  if (colours.length === 0) return "dark";
  const mean = colours.reduce((sum, hex) => sum + luminance(hex), 0) / colours.length;
  return mean > 0.35 ? "light" : "dark";
}
