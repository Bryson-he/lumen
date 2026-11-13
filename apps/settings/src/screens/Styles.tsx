// Component styles: how the built-in pieces (widget cards, the bar, the
// flyout, the launcher) are shaped, and which theme role each colour comes
// from. A style never holds a colour of its own, so it follows the theme.

import { open, save } from "@tauri-apps/plugin-dialog";
import { useId, useState, type CSSProperties } from "react";
import { hexToRgb } from "../color";
import { useLumen } from "../store";
import {
  ROLE_NAMES,
  STYLE_FONTS,
  STYLE_LIMITS,
  type ComponentStyle,
  type Roles,
  type StyleAssignments,
  type StyleBackground,
  type StyleRole,
  type StylesState,
  type Theme,
} from "../types";
import { Button, Check, Slider, Tip } from "../ui";

const STYLE_FILES = [{ name: "Lumen style", extensions: ["lumenstyle"] }];

const BACKGROUNDS: { value: StyleBackground; label: string }[] = [
  { value: "glass", label: "Glass (follows theme blur)" },
  { value: "solid", label: "Solid" },
  { value: "gradient", label: "Gradient" },
  { value: "none", label: "None (just the text and bars)" },
];

const PIECES: { key: keyof StyleAssignments; label: string }[] = [
  { key: "widgets", label: "Widgets" },
  { key: "bar", label: "Bar" },
  { key: "flyout", label: "Volume flyout" },
  { key: "launcher", label: "Launcher" },
];

const GLASS_OPACITY = { none: 1, low: 0.9, medium: 0.78, high: 0.64 } as const;

/** An id for a new style called `name` that nothing else uses (as `new_id` in lumen-ipc). */
function newId(name: string, taken: string[]): string {
  const slug =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 48) || "style";
  let id = slug;
  for (let n = 2; taken.includes(id); n++) id = `${slug}-${n}`;
  return id;
}

export function StylesView() {
  const { styles, setStyles, styleRequest, shown } = useLumen();
  const [pickedId, setPickedId] = useState("default");

  if (!styles) {
    return (
      <section className="card coming-soon">
        <p>Styles are not available right now.</p>
        <p className="muted">They are applied by the desktop widgets, which could not start. Restart Lumen from the tray and try again.</p>
      </section>
    );
  }

  const all = [...styles.builtin, ...styles.styles];
  const picked = all.find((style) => style.id === pickedId) ?? styles.builtin[0];
  const own = styles.styles.some((style) => style.id === picked.id);
  const users = PIECES.filter((piece) => styles.assigned[piece.key] === picked.id).map((piece) => piece.label.toLowerCase());

  const commit = (next: Pick<StylesState, "styles" | "assigned">) => void setStyles({ styles: next.styles, assigned: next.assigned });
  const change = (patch: Partial<ComponentStyle>) => commit({ styles: styles.styles.map((style) => (style.id === picked.id ? { ...style, ...patch } : style)), assigned: styles.assigned });

  const duplicate = () => {
    const name = own ? `${picked.name} copy` : `My ${picked.name.toLowerCase()}`;
    const copy = { ...picked, id: newId(name, all.map((style) => style.id)), name };
    commit({ styles: [...styles.styles, copy], assigned: styles.assigned });
    setPickedId(copy.id);
  };

  const remove = () => {
    const assigned = { ...styles.assigned };
    for (const piece of PIECES) if (assigned[piece.key] === picked.id) assigned[piece.key] = "default";
    commit({ styles: styles.styles.filter((style) => style.id !== picked.id), assigned });
    setPickedId("default");
  };

  const exportToFile = async () => {
    const path = await save({ title: `Export ${picked.name}`, defaultPath: `${picked.name}.lumenstyle`, filters: STYLE_FILES });
    if (path) await styleRequest("exportStyle", { id: picked.id, path });
  };

  const importFiles = async () => {
    const chosen = await open({ multiple: true, title: "Import styles", filters: STYLE_FILES });
    if (!chosen) return;
    for (const path of Array.isArray(chosen) ? chosen : [chosen]) {
      const added = await styleRequest<ComponentStyle>("importStyle", { path });
      if (added) setPickedId(added.id);
    }
  };

  const full = styles.styles.length >= STYLE_LIMITS.maxStyles;
  const everywhere = PIECES.every((piece) => styles.assigned[piece.key] === picked.id);
  const useEverywhere = () => commit({ styles: styles.styles, assigned: { widgets: picked.id, bar: picked.id, flyout: picked.id, launcher: picked.id } });

  return (
    <div className="split styles-split">
      <aside className="panel styles-list" aria-label="Styles">
        <h2 className="panel-title">Styles</h2>
        <StyleGroup label="Built in" styles={styles.builtin} picked={picked.id} assigned={styles.assigned} onPick={setPickedId} />
        {styles.styles.length > 0 && <StyleGroup label="Yours" styles={styles.styles} picked={picked.id} assigned={styles.assigned} onPick={setPickedId} />}
        <div className="button-row">
          <Button size="bar" kind="primary" disabled={full} onClick={duplicate}>
            Duplicate
          </Button>
          <Button size="bar" disabled={full} onClick={() => void importFiles()}>
            Import…
          </Button>
        </div>
        {styles.styles.length === 0 && (
          <Tip accent size={20}>
            duplicate one to make it yours
          </Tip>
        )}
      </aside>

      <section className="panel styles-editor" aria-label={`Style: ${picked.name}`}>
        <div className="field-row wrap">
          <h2 className="panel-title">{picked.name}</h2>
          <span className="header-actions">
            <Button size="bar" kind="primary" disabled={everywhere} onClick={useEverywhere}>
              {everywhere ? "In use everywhere" : "Use everywhere"}
            </Button>
            {!own && (
              <Button size="bar" disabled={full} onClick={duplicate}>
                Duplicate to edit
              </Button>
            )}
            <Button size="bar" onClick={() => void exportToFile()}>
              Export…
            </Button>
            {own && (
              <Button size="bar" kind="danger" onClick={remove}>
                Delete
              </Button>
            )}
          </span>
        </div>
        <div className="strip styles-status" role="status">
          <span className="status-dot" />
          {own ? (
            <span>
              <strong>Saved automatically.</strong> There is no Save button: every change is kept as you make it.{" "}
              {users.length > 0 ? `It is used by: ${users.join(", ")}, so changes show on your desktop straight away.` : "Nothing uses this style yet. Press “Use everywhere”, or choose it per piece under “Used by”."}
            </span>
          ) : (
            <span>
              <strong>Built-in style.</strong> {users.length > 0 ? `Used by: ${users.join(", ")}.` : "Press “Use everywhere” to put it on your desktop, or choose it per piece under “Used by”."} To change how it looks, press “Duplicate to edit”.
            </span>
          )}
        </div>
        <StyleFields key={picked.id} style={picked} roles={shown?.roles ?? null} disabled={!own} onChange={change} />
      </section>

      <div className="styles-side">
        <section className="panel" aria-label="Used by">
          <h2 className="panel-title">Used by</h2>
          {PIECES.map((piece) => (
            <AssignRow key={piece.key} label={piece.label} value={styles.assigned[piece.key]} styles={all} onChange={(id) => commit({ styles: styles.styles, assigned: { ...styles.assigned, [piece.key]: id } })} />
          ))}
          <p className="muted small-13">One widget can have its own style too: select it on the Widgets screen and look under Style.</p>
        </section>
        {shown && (
          <>
            <div className="label">Preview of “{picked.name}”</div>
            <StylePreview style={picked} theme={shown} />
          </>
        )}
      </div>
    </div>
  );
}

function StyleGroup({ label, styles, picked, assigned, onPick }: { label: string; styles: ComponentStyle[]; picked: string; assigned: StyleAssignments; onPick: (id: string) => void }) {
  const inUse = (id: string) => PIECES.some((piece) => assigned[piece.key] === id);
  return (
    <div className="stack-2">
      <span className="label">{label}</span>
      <ul className="styles-items">
        {styles.map((style) => (
          <li key={style.id}>
            <button type="button" className="styles-item" aria-pressed={style.id === picked} onClick={() => onPick(style.id)}>
              <span>{style.name}</span>
              {inUse(style.id) && <span className="muted small">in use</span>}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function AssignRow({ label, value, styles, onChange }: { label: string; value: string; styles: ComponentStyle[]; onChange: (id: string) => void }) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id} className="small-14">
        {label}
      </label>
      <select id={id} value={value} onChange={(event) => onChange(event.target.value)}>
        {styles.map((style) => (
          <option key={style.id} value={style.id}>
            {style.name}
          </option>
        ))}
      </select>
    </div>
  );
}

/** A choice among the theme's roles, with a dot in that role's current colour. */
function RoleField({ label, value, roles, disabled, onChange }: { label: string; value: StyleRole; roles: Roles | null; disabled: boolean; onChange: (role: StyleRole) => void }) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id} className="small-14">
        {label}
      </label>
      <span className="styles-role">
        <span className="dot styles-role-dot" aria-hidden="true" style={roles ? { background: roles[value] } : undefined} />
        <select id={id} value={value} disabled={disabled} onChange={(event) => onChange(event.target.value as StyleRole)}>
          {ROLE_NAMES.map(([role, label]) => (
            <option key={role} value={role}>
              {label}
            </option>
          ))}
        </select>
      </span>
    </div>
  );
}

function StyleFields({ style, roles, disabled, onChange }: { style: ComponentStyle; roles: Roles | null; disabled: boolean; onChange: (patch: Partial<ComponentStyle>) => void }) {
  const nameId = useId();
  const backgroundId = useId();
  const fontId = useId();
  const [name, setName] = useState(style.name);
  const hasFill = style.background === "solid" || style.background === "gradient";

  return (
    <div className="styles-fields">
      <div className="field styles-wide">
        <label htmlFor={nameId} className="small-14">
          Name
        </label>
        <input
          id={nameId}
          className="styles-name"
          type="text"
          maxLength={40}
          value={name}
          disabled={disabled}
          onChange={(event) => setName(event.target.value)}
          onBlur={() => (name.trim() ? name.trim() !== style.name && onChange({ name: name.trim() }) : setName(style.name))}
          onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
        />
      </div>

      <h3 className="label styles-wide">Background</h3>
      <div className="field">
        <label htmlFor={backgroundId} className="small-14">
          Kind
        </label>
        <select id={backgroundId} value={style.background} disabled={disabled} onChange={(event) => onChange({ background: event.target.value as StyleBackground })}>
          {BACKGROUNDS.map((item) => (
            <option key={item.value} value={item.value}>
              {item.label}
            </option>
          ))}
        </select>
      </div>
      {style.background !== "none" && <RoleField label="Colour" value={style.backgroundRole} roles={roles} disabled={disabled} onChange={(backgroundRole) => onChange({ backgroundRole })} />}
      {style.background === "gradient" && <RoleField label="Fades to" value={style.gradientRole} roles={roles} disabled={disabled} onChange={(gradientRole) => onChange({ gradientRole })} />}
      {hasFill && <Slider label="How solid" min={STYLE_LIMITS.opacity[0]} max={STYLE_LIMITS.opacity[1]} value={style.opacity} format={(v) => `${v}%`} disabled={disabled} onCommit={(opacity) => onChange({ opacity })} />}

      <h3 className="label styles-wide">Shape</h3>
      <div className="field">
        <Check checked={style.cornersFollowTheme} disabled={disabled} onChange={(cornersFollowTheme) => onChange({ cornersFollowTheme })}>
          Corners follow theme
        </Check>
        {!style.cornersFollowTheme && <Slider label="Corner radius" min={0} max={STYLE_LIMITS.radius} value={style.radius} format={(v) => `${v} px`} disabled={disabled} onCommit={(radius) => onChange({ radius })} />}
      </div>
      <div className="field">
        <Check checked={style.borderFollowsTheme} disabled={disabled} onChange={(borderFollowsTheme) => onChange({ borderFollowsTheme })}>
          Border follows theme
        </Check>
        {!style.borderFollowsTheme && <Slider label="Border width" min={0} max={STYLE_LIMITS.border} value={style.borderWidth} format={(v) => (v === 0 ? "none" : `${v} px`)} disabled={disabled} onCommit={(borderWidth) => onChange({ borderWidth })} />}
      </div>
      <RoleField label="Border colour" value={style.borderRole} roles={roles} disabled={disabled} onChange={(borderRole) => onChange({ borderRole })} />
      <Slider label="Spacing (bar, flyout, launcher)" min={STYLE_LIMITS.spacing[0]} max={STYLE_LIMITS.spacing[1]} step={5} value={style.spacing} format={(v) => `${v}%`} disabled={disabled} onCommit={(spacing) => onChange({ spacing })} />

      <h3 className="label styles-wide">Text and highlights</h3>
      <RoleField label="Text colour" value={style.textRole} roles={roles} disabled={disabled} onChange={(textRole) => onChange({ textRole })} />
      <RoleField label="Highlight colour" value={style.accentRole} roles={roles} disabled={disabled} onChange={(accentRole) => onChange({ accentRole })} />
      <div className="field">
        <label htmlFor={fontId} className="small-14">
          Font
        </label>
        <select id={fontId} value={style.font ?? ""} disabled={disabled} onChange={(event) => onChange({ font: event.target.value || null })}>
          <option value="">Theme's font</option>
          {STYLE_FONTS.map((font) => (
            <option key={font} value={font}>
              {font}
            </option>
          ))}
        </select>
      </div>
      <Slider label="Text size" min={STYLE_LIMITS.textScale[0]} max={STYLE_LIMITS.textScale[1]} step={5} value={style.textScale} format={(v) => `${v}%`} disabled={disabled} onCommit={(textScale) => onChange({ textScale })} />
    </div>
  );
}

/** A theme colour with an opacity, for the preview. */
function withAlpha(hex: string, alpha: number): string {
  const rgb = hexToRgb(hex);
  return rgb ? `rgb(${rgb[0]} ${rgb[1]} ${rgb[2]} / ${alpha})` : hex;
}

/** A widget card, a strip of bar and a flyout drawn in one style, in the theme's own colours. */
function StylePreview({ style, theme }: { style: ComponentStyle; theme: Theme }) {
  const r = theme.roles;
  const opacity = style.opacity / 100;
  const fill = r[style.backgroundRole];
  const background =
    style.background === "glass"
      ? withAlpha(fill, GLASS_OPACITY[theme.style.blur])
      : style.background === "solid"
        ? withAlpha(fill, opacity)
        : style.background === "gradient"
          ? `linear-gradient(135deg, ${withAlpha(fill, opacity)}, ${withAlpha(r[style.gradientRole], opacity)})`
          : "transparent";
  const vars = {
    "--s-background": background,
    "--s-radius": `${style.cornersFollowTheme ? theme.style.radius : style.radius}px`,
    "--s-border-width": `${style.borderFollowsTheme ? theme.style.border : style.borderWidth}px`,
    "--s-border": style.borderRole === "onSurface" ? withAlpha(r.onSurface, 0.14) : r[style.borderRole],
    "--s-text": r[style.textRole],
    "--s-accent": r[style.accentRole],
    "--s-track": withAlpha(r.outline, 0.55),
    "--s-font": `'${style.font ?? theme.style.font}', var(--font-body)`,
    "--s-text-scale": style.textScale / 100,
    "--s-spacing": style.spacing / 100,
    "--s-desktop-a": r.surface,
    "--s-desktop-b": r.neutral,
  } as CSSProperties;

  return (
    <div className="style-pv" style={vars} role="img" aria-label={`A widget, the bar and the flyout in the ${style.name} style`}>
      <div className="style-pv-bar">
        <span className="style-pv-strong">Lumen</span>
        <span>10:42</span>
        <span>
          <span className="style-pv-muted">CPU</span> 12%
        </span>
      </div>
      <div className="style-pv-pieces">
        <div className="style-pv-card">
          <span className="style-pv-muted">Thursday</span>
          <span className="style-pv-time">10:42</span>
        </div>
        <div className="style-pv-card style-pv-flyout">
          <span className="style-pv-muted">Volume</span>
          <span className="style-pv-track">
            <span />
          </span>
          <span>64%</span>
        </div>
      </div>
    </div>
  );
}
