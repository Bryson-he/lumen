// Theme & colours: the palette and style tokens every Lumen surface reads.

import { useEffect, useId, useRef, useState, type CSSProperties } from "react";
import { autoMode } from "../color";
import { PageHeader } from "../Shell";
import { useLumen } from "../store";
import { ROLE_NAMES, type Blur, type Mode, type Motion, type ExportsState, type Roles, type Style, type Theme, type Variant } from "../types";
import { Button, Check, Segmented, Slider, Tip, WallpaperArt } from "../ui";
import { StylesView } from "./Styles";

type SourceKind = "wallpaper" | "color" | "manual";

const VARIANTS: { value: Variant; label: string }[] = [
  { value: "tonal", label: "Tonal" },
  { value: "vibrant", label: "Vibrant" },
  { value: "expressive", label: "Expressive" },
  { value: "monochrome", label: "Monochrome" },
];
const CONTRAST = ["Reduced", "Standard", "High"];
const BLUR: Blur[] = ["none", "low", "medium", "high"];
const MOTION: Motion[] = ["slow", "normal", "fast"];
const FONTS = [
  { value: "Inter", label: "Inter" },
  { value: "Segoe UI Variable", label: "Segoe UI Variable" },
  { value: "Space Grotesk", label: "Space Grotesk" },
  { value: "Caveat", label: "Caveat (sketch)" },
];
const capital = (word: string) => word[0].toUpperCase() + word.slice(1);

export function ThemeScreen() {
  const { shown, wallpapers, wallpaperOf, previewTheme, go, themeView: view, setThemeView: setView } = useLumen();
  // Which source tab is open; follows the theme until the user picks another.
  const [picked, setPicked] = useState<SourceKind | null>(null);
  // "Auto" stays highlighted until Light or Dark is chosen by hand.
  const [auto, setAuto] = useState(false);
  const sourceKind: SourceKind = picked ?? shown?.source.type ?? "wallpaper";
  useEffect(() => setPicked(null), [shown?.source.type]);

  const wallpaper = wallpaperOf(shown);
  const manual = sourceKind === "manual";

  const chooseSource = (kind: SourceKind) => {
    setPicked(kind);
    // Manual keeps today's colours and stops generating: mark the theme so.
    if (kind === "manual" && shown && shown.source.type !== "manual") {
      void previewTheme({ theme: { ...shown, source: { type: "manual" } } });
    }
  };

  const setStyle = (changes: Partial<Style>) => shown && void previewTheme({ style: { ...shown.style, ...changes } });

  const setMode = (choice: Mode | "auto") => {
    if (!shown) return;
    setAuto(choice === "auto");
    const mode = choice === "auto" ? autoMode(wallpaper?.palette ?? (shown.source.type === "color" ? [shown.source.value] : [shown.roles.surface])) : choice;
    void previewTheme(manual ? { theme: { ...shown, mode } } : { mode });
  };

  return (
    <>
      <PageHeader title="Theme & colours" tip={view === "colours" ? "every widget, bar and menu reads from these" : "shape each piece your way; the colours still follow the theme"}>
        <div className="header-actions header-actions-center">
          <Segmented
            label="What to edit"
            look="track"
            value={view}
            onChange={setView}
            options={[
              { value: "colours", label: "Colours" },
              { value: "styles", label: "Component styles" },
            ]}
          />
          {view === "colours" && (
            <Segmented
              label="Colour source"
              look="track"
              value={sourceKind}
              onChange={chooseSource}
              options={[
                { value: "wallpaper", label: "From wallpaper" },
                { value: "color", label: "Pick a colour" },
                { value: "manual", label: "Manual", disabled: !shown },
              ]}
            />
          )}
        </div>
      </PageHeader>

      {view === "styles" && <StylesView />}
      <div className="theme-columns" hidden={view === "styles"}>
        <div className="theme-left">
          {sourceKind === "wallpaper" && (
            <section className="panel" aria-label="Source wallpaper">
              <div className="label">Source wallpaper</div>
              {wallpaper ? (
                <>
                  <WallpaperArt className="source-art" wallpaper={wallpaper} label={`${wallpaper.name} wallpaper`} />
                  <div className="field-row wrap">
                    <span className="strong">{wallpaper.name}</span>
                    <button type="button" className="link link-accent" onClick={() => go("wallpapers")}>
                      Change
                    </button>
                  </div>
                </>
              ) : (
                <>
                  <p className="muted small-14">{wallpapers.length ? "Choose a wallpaper to build the colours from." : "Import a wallpaper to build the colours from."}</p>
                  <Button onClick={() => go("wallpapers")}>Choose a wallpaper</Button>
                </>
              )}
            </section>
          )}
          {sourceKind === "color" && (
            <ColourSource value={shown?.source.type === "color" ? shown.source.value : (shown?.roles.primary ?? null)} onPick={(value) => void previewTheme({ source: { type: "color", value } })} />
          )}
          {manual && (
            <section className="panel" aria-label="Manual colours">
              <div className="label">Manual colours</div>
              <p className="muted small-14">Lumen stops generating. Click any swatch to set that colour yourself.</p>
            </section>
          )}

          <section className="panel" aria-label="Scheme variant">
            <div className="label">Scheme variant</div>
            <Segmented label="Scheme variant" look="grid-2" value={shown && !manual ? shown.variant : null} onChange={(variant) => void previewTheme({ variant })} options={VARIANTS.map((v) => ({ ...v, disabled: !shown || manual }))} />
            <Slider
              label="Contrast"
              min={0}
              max={2}
              value={(shown?.contrast ?? 0) + 1}
              format={(v) => CONTRAST[v]}
              disabled={!shown || manual}
              onCommit={(v) => void previewTheme({ contrast: v - 1 })}
            />
            <div className="field">
              <div className="small-14">Mode</div>
              <Segmented
                label="Light or dark mode"
                look="grid-3"
                value={shown ? (auto ? "auto" : shown.mode) : null}
                onChange={setMode}
                options={[
                  { value: "light", label: "Light", disabled: !shown },
                  { value: "dark", label: "Dark", disabled: !shown },
                  { value: "auto", label: "Auto", disabled: !shown },
                ]}
              />
            </div>
          </section>
        </div>

        <section className="panel theme-centre" aria-label="Colour roles">
          <div className="field-row wrap baseline">
            <div className="label">Colour roles</div>
            {shown && (
              <Tip accent size={20}>
                click any swatch to fine-tune
              </Tip>
            )}
          </div>
          {shown ? (
            <>
              <div className="role-grid">
                {ROLE_NAMES.map(([key, name]) => (
                  <RoleSwatch key={key} name={name} value={shown.roles[key]} onChange={(hex) => void previewTheme({ theme: { ...shown, roles: { ...shown.roles, [key]: hex } as Roles } })} />
                ))}
              </div>
              <div className="label live-label">Live preview</div>
              <LivePreview theme={shown} />
            </>
          ) : (
            <div className="empty">
              <p>No colours yet.</p>
              <p className="muted">Start from a wallpaper or pick a colour, and the eight roles appear here.</p>
            </div>
          )}
        </section>

        <section className="panel theme-right" aria-label="Style">
          <div className="label">Style</div>
          <Slider label="Corner radius" min={0} max={24} value={shown?.style.radius ?? 12} format={(v) => `${v} px`} disabled={!shown} onCommit={(radius) => setStyle({ radius })} />
          <Slider
            label="Blur / glass"
            min={0}
            max={3}
            value={BLUR.indexOf(shown?.style.blur ?? "medium")}
            format={(v) => capital(BLUR[v])}
            disabled={!shown}
            onCommit={(v) => setStyle({ blur: BLUR[v] })}
          />
          <Slider label="Border" min={0} max={3} value={shown?.style.border ?? 1} format={(v) => `${v} px`} disabled={!shown} onCommit={(border) => setStyle({ border })} />
          <FontSelect value={shown?.style.font ?? "Inter"} disabled={!shown} onChange={(font) => setStyle({ font })} />
          <Slider
            label="Animation speed"
            min={0}
            max={2}
            value={MOTION.indexOf(shown?.style.motion ?? "normal")}
            format={(v) => capital(MOTION[v])}
            disabled={!shown}
            onCommit={(v) => setStyle({ motion: MOTION[v] })}
          />
          <Check checked={shown?.setWindowsAccent ?? false} disabled={!shown} onChange={(setWindowsAccent) => void previewTheme({ setWindowsAccent })}>
            Also set the Windows accent colour
          </Check>
          <OtherApps />
        </section>
      </div>
    </>
  );
}

/** The kept theme written out as files for other apps, and the Windows Terminal colour scheme. */
function OtherApps() {
  const { request, theme } = useLumen();
  const [state, setState] = useState<ExportsState | null>(null);
  // The files are rewritten whenever a theme is kept.
  const kept = theme ? JSON.stringify([theme.roles, theme.mode, theme.style]) : "";
  useEffect(() => {
    let current = true;
    void request<ExportsState>("getExports").then((result) => {
      if (current && result) setState(result);
    });
    return () => {
      current = false;
    };
  }, [request, kept]);
  if (!state) return null;

  return (
    <div className="other-apps">
      <div className="label">Other apps</div>
      <p className="muted small-13">
        {state.files.length > 0 ? "Each theme you keep is also written out as files: a Windows Terminal colour scheme, VS Code colours, CSS variables and JSON." : "Keep a theme and Lumen writes its colours out as files for Windows Terminal, VS Code, CSS and JSON."}
      </p>
      <Button size="bar" onClick={() => void request("openExportsFolder")}>
        Open the folder
      </Button>
      <Check checked={state.terminal} disabled={!state.terminalAvailable} onChange={(terminal) => void request<ExportsState>("setExports", { terminal }).then((result) => result && setState(result))}>
        Add a Lumen colour scheme to Windows Terminal
      </Check>
      <p className="muted small">
        {!state.terminalAvailable
          ? "Windows Terminal was not found on this PC."
          : state.terminal
            ? "In Terminal, choose Settings, a profile, Appearance, Colour scheme, then Lumen. It follows your theme from then on. Open tabs pick up a new theme when Terminal restarts."
            : "Adds one file to Terminal's own add-ons folder. Your Terminal settings are not changed, and switching this off removes the file."}
      </p>
    </div>
  );
}

/** "Pick a colour": one colour in, a whole palette out. */
function ColourSource({ value, onPick }: { value: string | null; onPick: (hex: string) => void }) {
  const id = useId();
  const [local, setLocal] = useState(value ?? "");
  const timer = useRef<number>();
  useEffect(() => setLocal(value ?? ""), [value]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  return (
    <section className="panel" aria-label="Source colour">
      <label className="label" htmlFor={id}>
        Source colour
      </label>
      <div className="colour-source">
        <input
          id={id}
          type="color"
          className="colour-well"
          value={/^#[0-9a-f]{6}$/i.test(local) ? local : "#808080"}
          onChange={(event) => {
            const next = event.target.value.toUpperCase();
            setLocal(next);
            // The picker fires continuously while dragging; wait for a pause.
            window.clearTimeout(timer.current);
            timer.current = window.setTimeout(() => onPick(next), 160);
          }}
        />
        <span className="hex">{local || "none yet"}</span>
      </div>
      <p className="muted small-14">Lumen builds all eight roles from this one colour.</p>
    </section>
  );
}

/** One colour role: swatch, name and hex. Clicking the swatch opens the colour picker. */
function RoleSwatch({ name, value, onChange }: { name: string; value: string; onChange: (hex: string) => void }) {
  const timer = useRef<number>();
  const [local, setLocal] = useState(value);
  useEffect(() => setLocal(value), [value]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  return (
    <label className="role">
      <span className="role-swatch" style={{ background: local }}>
        <input
          type="color"
          value={local}
          aria-label={`${name} colour, ${local}`}
          onChange={(event) => {
            const next = event.target.value.toUpperCase();
            setLocal(next);
            window.clearTimeout(timer.current);
            timer.current = window.setTimeout(() => onChange(next), 160);
          }}
        />
      </span>
      <span className="role-name">{name}</span>
      <span className="hex muted">{local}</span>
    </label>
  );
}

function FontSelect({ value, disabled, onChange }: { value: string; disabled: boolean; onChange: (font: string) => void }) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id} className="small-14">
        Font
      </label>
      <select id={id} value={value} disabled={disabled} onChange={(event) => onChange(event.target.value)}>
        {FONTS.map((font) => (
          <option key={font.value} value={font.value}>
            {font.label}
          </option>
        ))}
        {!FONTS.some((font) => font.value === value) && <option value={value}>{value}</option>}
      </select>
    </div>
  );
}

/** A small status bar and two widgets, drawn in the theme being edited. */
function LivePreview({ theme }: { theme: Theme }) {
  const r = theme.roles;
  // The theme's own colours and style, handed to CSS as variables for this box only.
  const vars = {
    "--t-primary": r.primary,
    "--t-secondary": r.secondary,
    "--t-tertiary": r.tertiary,
    "--t-neutral": r.neutral,
    "--t-surface": r.surface,
    "--t-on-surface": r.onSurface,
    "--t-outline": r.outline,
    "--t-accent": r.accent,
    "--t-radius": `${theme.style.radius}px`,
    "--t-border": `${theme.style.border}px`,
    "--t-font": `'${theme.style.font}', var(--font-body)`,
  } as CSSProperties;

  return (
    <div className="live" style={vars} aria-label="Preview of a status bar and two widgets in this theme" role="img">
      <div className="live-bar">
        <span className="strong">Lumen</span>
        <span className="live-bar-right">
          <span>CPU 12%</span>
          <span>10:42</span>
        </span>
      </div>
      <div className="live-widgets">
        <div className="live-card live-card-wide">
          <div className="live-album" />
          <div className="live-track">
            <span className="strong">Now playing</span>
            <span className="live-muted">Track title · Artist</span>
            <div className="live-progress">
              <div />
            </div>
          </div>
        </div>
        <div className="live-card live-card-clock">
          <span className="live-muted">Thursday</span>
          <span className="live-time">10:42</span>
          <span className="live-accent-dot" />
        </div>
      </div>
    </div>
  );
}
