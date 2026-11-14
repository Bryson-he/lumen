// The Parts tab of the widget inspector: every piece of a widget (each line
// of text, each bar, the picture) with its own font, size, colour, place and
// visibility. Colours are theme roles, so they keep following the theme.

import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useId, useState } from "react";
import { useLumen } from "../store";
import { PART_COLORS, PART_ICONS, PART_LIMITS, ROLE_NAMES, WIDGET_NAMES, type WidgetOption, type Part, type PartAlign, type PartColor, type PartWeight, type Placement, type Roles, type GalleryOffer, type UpdateWidgetParams } from "../types";
import { Button, Check, Segmented, Slider } from "../ui";
import { allSources, PART_KIND_NAMES, partsOf } from "../widgetView";
import { ScriptCard } from "./Scripts";

const FONT_FILES = [{ name: "Font files", extensions: ["ttf", "otf", "ttc", "woff", "woff2"] }];

export function PartsTab({
  placement,
  locked,
  openId,
  designing,
  onOpen,
  onDesign,
  onUpdate,
}: {
  placement: Placement;
  locked: boolean;
  /** The part whose fields are showing (also the one selected in the designer). */
  openId: string | null;
  designing: boolean;
  onOpen: (id: string | null) => void;
  onDesign: () => void;
  onUpdate: (params: UpdateWidgetParams) => void;
}) {
  const { partDefaults, fonts, refreshFonts, importFonts, shown, widgetFiles, scripts, request, widgets } = useLumen();
  const [naming, setNaming] = useState<string | null>(null);
  const [offer, setOffer] = useState<GalleryOffer | null>(null);
  const setOpenId = onOpen;
  useEffect(() => void refreshFonts(), [refreshFonts]);

  if (!partDefaults) return <p className="muted small-13">The parts of this widget are not available right now.</p>;
  const { parts, file, own } = partsOf(placement, partDefaults, widgetFiles, widgets?.resolved);

  const exportPackage = async () => {
    if (!placement.file || !file) return;
    const path = await saveDialog({ title: `Share ${file.name}`, defaultPath: `${file.name}.lumenwidget`, filters: [{ name: "Lumen widget", extensions: ["lumenwidget"] }] });
    if (path) await request<string>("exportWidgetPackage", { file: placement.file, path });
  };

  const offerToGallery = async () => {
    if (!placement.file) return;
    const made = await request<GalleryOffer>("offerToGallery", { file: placement.file });
    if (made) setOffer(made);
  };

  const saveAsFile = async () => {
    const name = (naming ?? "").trim();
    if (!name) return;
    const made = await request<string>("saveWidgetAsFile", { id: placement.id, name });
    if (made) setNaming(null);
  };
  const save = (next: Part[]) => onUpdate({ id: placement.id, parts: next });
  const change = (partId: string, patch: Partial<Part>) => save(parts.map((part) => (part.id === partId ? { ...part, ...patch } : part)));

  const addFonts = async () => {
    const chosen = await open({ multiple: true, title: "Add fonts to Lumen", filters: FONT_FILES });
    if (chosen) await importFonts(Array.isArray(chosen) ? chosen : [chosen]);
  };

  return (
    <>
      {!designing && (
        <Button size="bar" kind="primary" onClick={onDesign}>
          Design with the mouse
        </Button>
      )}
      {placement.file ? (
        <div className={`strip parts-file${file?.error ? " strip-danger" : ""}`} role={file?.error ? "alert" : undefined}>
          <span>
            {file ? (
              <>
                Drawn from the file <strong>{placement.file}</strong>.{" "}
                {file.error ? `It has a mistake${file.errorLine ? ` on line ${file.errorLine}` : ""}: ${file.error} Showing the last version that worked.` : "Changes here are written to it."}{" "}
                {!file.error && (
                  <button type="button" className="link small-13" onClick={() => void exportPackage()}>
                    Share it as one file…
                  </button>
                )}{" "}
                {!file.error && (
                  <button type="button" className="link small-13" onClick={() => void offerToGallery()}>
                    Offer it to the online gallery…
                  </button>
                )}
                {offer && (
                  <span className="parts-offer" role="status">
                    Packed as <strong>{offer.path}</strong>. A page has opened in your browser: attach that file there and send it. It is listed for everyone once it has been looked at. Nothing was uploaded by Lumen.
                  </span>
                )}
              </>
            ) : (
              <>
                The file <strong>{placement.file}</strong> is missing from the widgets folder, so this widget is empty.
              </>
            )}
          </span>
        </div>
      ) : null}
      {placement.file && scripts.filter((script) => script.file === placement.file).map((script) => <ScriptCard key={script.file} script={script} />)}
      {placement.file ? null : naming === null ? (
        <button type="button" className="link small-13 parts-save-as" onClick={() => setNaming(WIDGET_NAMES[placement.kind])}>
          Save as a widget file (to edit as text or reuse)…
        </button>
      ) : (
        <div className="parts-naming">
          <input
            className="parts-text"
            type="text"
            aria-label="Name for the widget file"
            maxLength={60}
            autoFocus
            value={naming}
            onChange={(event) => setNaming(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void saveAsFile();
              if (event.key === "Escape") setNaming(null);
            }}
          />
          <Button size="bar" kind="primary" disabled={!naming.trim()} onClick={() => void saveAsFile()}>
            Save
          </Button>
          <Button size="bar" onClick={() => setNaming(null)}>
            Cancel
          </Button>
        </div>
      )}
      {file && file.options.length > 0 && <OptionsPanel options={file.options} answers={placement.options ?? {}} locked={locked} onChange={(options) => onUpdate({ id: placement.id, options })} />}
      <p className="muted small-13">Each piece has its own font, size, colour and place. Changes show on your desktop straight away and are saved as you go.</p>
      <ul className="parts-list">
        {parts.map((part) => {
          const isOpen = part.id === openId;
          return (
            <li key={part.id} className={`parts-item${isOpen ? " parts-item-open" : ""}${part.hidden ? " parts-item-hidden" : ""}`}>
              <div className="parts-row">
                <button type="button" className="parts-name" aria-expanded={isOpen} onClick={() => setOpenId(isOpen ? null : part.id)}>
                  <span>{part.name}</span>
                  <span className="muted small">{part.hidden ? "hidden" : PART_KIND_NAMES[part.kind]}</span>
                </button>
                <button type="button" className="link small-13" disabled={locked} onClick={() => change(part.id, { hidden: !part.hidden })}>
                  {part.hidden ? "Show" : "Hide"}
                </button>
              </div>
              {isOpen && <PartFields part={part} roles={shown?.roles ?? null} fonts={fonts} sources={allSources(partDefaults)} locked={locked || !!(placement.file && !file)} onChange={(patch) => change(part.id, patch)} />}
            </li>
          );
        })}
      </ul>
      <div className="button-row">
        <Button size="bar" onClick={() => void addFonts()}>
          Add a font file…
        </Button>
        {!placement.file && (
          <Button size="bar" disabled={!own || locked} onClick={() => save([])}>
            Reset parts
          </Button>
        )}
      </div>
      {!own && <span className="muted small-13">This widget uses the arrangement it came with.</span>}
    </>
  );
}

function PartFields({
  part,
  roles,
  fonts,
  sources,
  locked,
  onChange,
}: {
  part: Part;
  roles: Roles | null;
  fonts: ReturnType<typeof useLumen>["fonts"];
  sources: { source: string; label: string }[];
  locked: boolean;
  onChange: (patch: Partial<Part>) => void;
}) {
  const showsId = useId();
  const iconId = useId();
  const clickId = useId();
  const alertId = useId();
  const clickKind = part.click.startsWith("open ") ? "open" : part.click === "media.toggle" || part.click === "lumen.open" || part.click === "script" ? part.click : part.click ? "open" : "";
  const fontId = useId();
  const colourId = useId();
  const textId = useId();
  const [text, setText] = useState(part.text);
  useEffect(() => setText(part.text), [part.text]);
  const isText = part.kind === "text";
  // The colour a named role has right now, for the dot beside the choice.
  const swatch = roles && part.color in roles ? roles[part.color as keyof Roles] : null;
  // A font that is no longer installed still shows as the current choice.
  const known = fonts ? [...fonts.bundled, ...fonts.imported, ...fonts.installed] : [];

  return (
    <div className="parts-fields">
      {isText && part.source === "" && (
        <div className="field">
          <label htmlFor={textId} className="small-14">
            Text
          </label>
          <span className="muted small">
            Mix in live values with braces:{" "}
            {sources
              .filter((item) => item.source !== "media.art")
              .map((item) => `{${item.source}}`)
              .join("  ")}
          </span>
          <input
            id={textId}
            className="parts-text"
            type="text"
            maxLength={200}
            value={text}
            disabled={locked}
            onChange={(event) => setText(event.target.value)}
            onBlur={() => text !== part.text && onChange({ text })}
            onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
          />
        </div>
      )}
      {part.kind === "icon" && (
        <div className="field">
          <label htmlFor={iconId} className="small-14">
            Icon
          </label>
          <input
            id={iconId}
            className="parts-text"
            type="text"
            list={`${iconId}-names`}
            maxLength={40}
            defaultValue={part.text}
            key={part.text}
            disabled={locked}
            onBlur={(event) => event.target.value.trim() && event.target.value !== part.text && onChange({ text: event.target.value.trim() })}
            onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
          />
          <datalist id={`${iconId}-names`}>
            {Object.keys(PART_ICONS).map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          <span className="muted small">A name from the list (home, music, star, cloud…), or any emoji or letter.</span>
        </div>
      )}
      {(part.kind === "bar" || part.kind === "ring" || part.kind === "graph") && (
        <div className="field">
          <label htmlFor={showsId} className="small-14">
            Shows
          </label>
          <select id={showsId} value={part.source} disabled={locked} onChange={(event) => onChange({ source: event.target.value })}>
            {!sources.some((item) => item.source === part.source) && <option value={part.source}>{part.source || "Nothing"}</option>}
            {sources
              .filter((item) => item.source.startsWith("stats.") || (part.kind !== "graph" && (item.source === "media.progress" || item.source === "weather.humidity")))
              .map((item) => (
                <option key={item.source} value={item.source}>
                  {item.label}
                </option>
              ))}
          </select>
          {part.kind === "graph" && <span className="muted small">How the value has moved over the last two minutes.</span>}
        </div>
      )}
      {(part.kind === "shape" || part.kind === "image") && (
        <Slider label="Corner radius" min={0} max={60} value={Math.round(part.radius)} format={(v) => `${v} px`} disabled={locked} onCommit={(radius) => onChange({ radius })} />
      )}
      {isText && (
        <>
          <div className="field">
            <label htmlFor={fontId} className="small-14">
              Font
            </label>
            <select id={fontId} value={part.font ?? ""} disabled={locked} onChange={(event) => onChange({ font: event.target.value || null })}>
              <option value="">Same as the widget</option>
              {part.font && !known.includes(part.font) && <option value={part.font}>{part.font} (not installed)</option>}
              {fonts && (
                <>
                  <optgroup label="Comes with Lumen">
                    {fonts.bundled.map((font) => (
                      <option key={font}>{font}</option>
                    ))}
                  </optgroup>
                  {fonts.imported.length > 0 && (
                    <optgroup label="Added by you">
                      {fonts.imported.map((font) => (
                        <option key={font}>{font}</option>
                      ))}
                    </optgroup>
                  )}
                  <optgroup label="Installed on this PC">
                    {fonts.installed.map((font) => (
                      <option key={font}>{font}</option>
                    ))}
                  </optgroup>
                </>
              )}
            </select>
          </div>
          <Slider label="Text size" min={PART_LIMITS.textSize[0]} max={PART_LIMITS.textSize[1]} value={Math.round(part.size)} format={(v) => `${v} px`} disabled={locked} onCommit={(size) => onChange({ size })} />
          <div className="field">
            <span className="small-14">Weight</span>
            <Segmented<PartWeight>
              label="Weight"
              look="inset"
              value={part.weight}
              onChange={(weight) => onChange({ weight })}
              options={[
                { value: "regular", label: "Regular", disabled: locked },
                { value: "medium", label: "Medium", disabled: locked },
                { value: "semiBold", label: "Bold", disabled: locked },
              ]}
            />
          </div>
          <div className="field">
            <span className="small-14">Align</span>
            <Segmented<PartAlign>
              label="Align"
              look="inset"
              value={part.align}
              onChange={(align) => onChange({ align })}
              options={[
                { value: "left", label: "Left", disabled: locked },
                { value: "centre", label: "Centre", disabled: locked },
                { value: "right", label: "Right", disabled: locked },
              ]}
            />
          </div>
        </>
      )}
      {part.kind !== "image" && (
        <div className="field">
          <label htmlFor={colourId} className="small-14">
            Colour
          </label>
          <span className="styles-role">
            {swatch && <span className="dot styles-role-dot" aria-hidden="true" style={{ background: swatch }} />}
            <select id={colourId} value={part.color} disabled={locked} onChange={(event) => onChange({ color: event.target.value as PartColor })}>
              <optgroup label="From the widget's style">
                {PART_COLORS.map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </optgroup>
              <optgroup label="A theme colour">
                {ROLE_NAMES.map(([role, label]) => (
                  <option key={role} value={role}>
                    {label}
                  </option>
                ))}
              </optgroup>
            </select>
          </span>
        </div>
      )}
      <Slider label="Opacity" min={10} max={100} step={5} value={part.opacity} format={(v) => `${v}%`} disabled={locked} onCommit={(opacity) => onChange({ opacity })} />
      <div className="field-grid">
        <PartNumber label="X" value={part.x} min={-2000} max={2000} disabled={locked} onCommit={(x) => onChange({ x })} />
        <PartNumber label="Y" value={part.y} min={-2000} max={2000} disabled={locked} onCommit={(y) => onChange({ y })} />
        <PartNumber label="Width" value={part.width} min={1} max={2000} disabled={locked} onCommit={(width) => onChange({ width })} />
        <PartNumber label="Height" value={part.height} min={1} max={2000} disabled={locked} onCommit={(height) => onChange({ height })} />
      </div>
      <Check checked={part.stretch} disabled={locked} onChange={(stretch) => onChange({ stretch })}>
        Gets wider with the widget
      </Check>
      <Check checked={part.fromBottom} disabled={locked} onChange={(fromBottom) => onChange({ fromBottom })}>
        Stays near the bottom edge
      </Check>

      <h3 className="label parts-heading">What it does</h3>
      <div className="field">
        <label htmlFor={clickId} className="small-14">
          When clicked
        </label>
        <select id={clickId} value={clickKind} disabled={locked} onChange={(event) => onChange({ click: event.target.value === "open" ? "open " : event.target.value })}>
          <option value="">Nothing</option>
          <option value="open">Open a program, file or web page</option>
          <option value="media.toggle">Play or pause</option>
          <option value="lumen.open">Open Lumen</option>
          <option value="script">Tell the widget's script</option>
        </select>
        {clickKind === "open" && (
          <span className="parts-naming">
            <LineField label="What to open" placeholder="https://… or C:\…\app.exe" value={part.click.replace(/^open\s*/, "")} disabled={locked} onCommit={(target) => onChange({ click: target ? `open ${target}` : "" })} />
            <Button
              size="bar"
              disabled={locked}
              onClick={() =>
                void open({ title: "Choose what to open" }).then((chosen) => {
                  if (typeof chosen === "string") onChange({ click: `open ${chosen}` });
                })
              }
            >
              Choose…
            </Button>
          </span>
        )}
        {part.click && <span className="muted small">A widget with a clickable part takes mouse clicks anywhere on it, so what is underneath cannot be clicked through it.</span>}
      </div>

      <h3 className="label parts-heading">When it shows</h3>
      <LineField label="Only show when" placeholder="always (for example: stats.cpu > 90)" value={part.when} disabled={locked} onCommit={(when) => onChange({ when })} />
      <span className="muted small">
        Compare a live value with a number (<code>stats.cpu &gt; 90</code>, <code>weather.temp &lt; 0</code>), or just name one (<code>media.playing</code>, or <code>!media.playing</code> for the opposite).
      </span>
      <Check checked={part.hoverOnly} disabled={locked} onChange={(hoverOnly) => onChange({ hoverOnly })}>
        Only while the mouse is over the widget
      </Check>

      {part.kind !== "image" && (
        <>
          <h3 className="label parts-heading">Change colour</h3>
          <LineField label="Change colour when" placeholder="never (for example: stats.cpu > 90)" value={part.alertWhen} disabled={locked} onCommit={(alertWhen) => onChange({ alertWhen })} />
          {part.alertWhen && (
            <div className="field">
              <label htmlFor={alertId} className="small-14">
                To this colour
              </label>
              <select id={alertId} value={part.alertColor} disabled={locked} onChange={(event) => onChange({ alertColor: event.target.value as PartColor })}>
                {[...PART_COLORS, ...ROLE_NAMES].map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </div>
          )}
        </>
      )}
    </div>
  );
}

type Answers = Record<string, number | boolean | string>;

/** The options this widget's file exposes, each with a control. The answers belong to this one widget. */
function OptionsPanel({ options, answers, locked, onChange }: { options: WidgetOption[]; answers: Answers; locked: boolean; onChange: (answers: Answers) => void }) {
  const baseId = useId();
  const set = (name: string, value: number | boolean | string) => onChange({ ...answers, [name]: value });
  const changed = Object.keys(answers).length > 0;
  return (
    <section className="parts-options" aria-label="This widget's options">
      <div className="field-row">
        <h3 className="label">Options</h3>
        {changed && (
          <button type="button" className="link small-13" disabled={locked} onClick={() => onChange({})}>
            Back to defaults
          </button>
        )}
      </div>
      {options.map((option) => {
        const value = answers[option.name] ?? option.default;
        const id = `${baseId}-${option.name}`;
        if (option.kind === "toggle") {
          return (
            <Check key={option.name} checked={value === true} disabled={locked} onChange={(on) => set(option.name, on)}>
              {option.label}
            </Check>
          );
        }
        if (option.kind === "number") {
          const [min, max] = [option.min ?? 0, option.max ?? 100];
          return <Slider key={option.name} label={option.label} min={min} max={max} step={max - min <= 2 ? 0.05 : 1} value={Number(value)} format={(v) => String(Math.round(v * 100) / 100)} disabled={locked} onCommit={(next) => set(option.name, next)} />;
        }
        if (option.kind === "text") return <LineField key={option.name} label={option.label} value={String(value)} disabled={locked} onCommit={(next) => set(option.name, next)} />;
        return (
          <div key={option.name} className="field">
            <label htmlFor={id} className="small-14">
              {option.label}
            </label>
            <select id={id} value={String(value)} disabled={locked} onChange={(event) => set(option.name, event.target.value)}>
              {option.kind === "choice"
                ? option.choices.map((choice) => <option key={choice}>{choice}</option>)
                : [...PART_COLORS, ...ROLE_NAMES].map(([colour, label]) => (
                    <option key={colour} value={colour}>
                      {label}
                    </option>
                  ))}
            </select>
          </div>
        );
      })}
      <span className="muted small">These belong to this widget only. Another widget from the same file keeps its own.</span>
    </section>
  );
}

/** One line of text that is saved when the box is left or Enter is pressed. */
function LineField({ label, value, placeholder, disabled, onCommit }: { label: string; value: string; placeholder?: string; disabled?: boolean; onCommit: (value: string) => void }) {
  const id = useId();
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <div className="field parts-line">
      <label htmlFor={id} className="small-14">
        {label}
      </label>
      <input
        id={id}
        className="parts-text"
        type="text"
        maxLength={300}
        placeholder={placeholder}
        value={text}
        disabled={disabled}
        onChange={(event) => setText(event.target.value)}
        onBlur={() => text.trim() !== value && onCommit(text.trim())}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          if (event.key === "Escape") setText(value);
        }}
      />
    </div>
  );
}

function PartNumber({ label, value, min, max, disabled, onCommit }: { label: string; value: number; min: number; max: number; disabled?: boolean; onCommit: (value: number) => void }) {
  const [text, setText] = useState(String(Math.round(value)));
  useEffect(() => setText(String(Math.round(value))), [value]);
  const commit = () => {
    const parsed = Number(text);
    if (text.trim() === "" || !Number.isFinite(parsed)) {
      setText(String(Math.round(value)));
      return;
    }
    const clamped = Math.min(Math.max(Math.round(parsed), min), max);
    setText(String(clamped));
    if (clamped !== Math.round(value)) onCommit(clamped);
  };
  return (
    <label className="number-field">
      {label}
      <input
        value={text}
        inputMode="numeric"
        disabled={disabled}
        onChange={(event) => setText(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
          if (event.key === "Escape") setText(String(Math.round(value)));
        }}
      />
    </label>
  );
}
