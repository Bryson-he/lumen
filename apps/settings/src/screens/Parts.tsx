// The Parts tab of the widget inspector: every piece of a widget (each line
// of text, each bar, the picture) with its own font, size, colour, place and
// visibility. Colours are theme roles, so they keep following the theme.

import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useId, useState } from "react";
import { useLumen } from "../store";
import { PART_COLORS, PART_LIMITS, ROLE_NAMES, type Part, type PartAlign, type PartColor, type PartWeight, type Placement, type Roles, type UpdateWidgetParams } from "../types";
import { Button, Check, Segmented, Slider } from "../ui";
import { allSources, PART_KIND_NAMES } from "../widgetView";

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
  const { partDefaults, fonts, refreshFonts, importFonts, shown } = useLumen();
  const setOpenId = onOpen;
  useEffect(() => void refreshFonts(), [refreshFonts]);

  const defaults = partDefaults?.[placement.kind];
  if (!defaults) return <p className="muted small-13">The parts of this widget are not available right now.</p>;

  const own = !!placement.parts;
  const parts = placement.parts ?? defaults.parts;
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
              {isOpen && <PartFields part={part} roles={shown?.roles ?? null} fonts={fonts} sources={allSources(partDefaults)} locked={locked} onChange={(patch) => change(part.id, patch)} />}
            </li>
          );
        })}
      </ul>
      <div className="button-row">
        <Button size="bar" onClick={() => void addFonts()}>
          Add a font file…
        </Button>
        <Button size="bar" disabled={!own || locked} onClick={() => save([])}>
          Reset parts
        </Button>
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
      {part.kind === "bar" && (
        <div className="field">
          <label htmlFor={showsId} className="small-14">
            Shows
          </label>
          <select id={showsId} value={part.source} disabled={locked} onChange={(event) => onChange({ source: event.target.value })}>
            {!sources.some((item) => item.source === part.source) && <option value={part.source}>{part.source || "Nothing"}</option>}
            {sources
              .filter((item) => item.source.startsWith("stats.") || item.source === "media.progress")
              .map((item) => (
                <option key={item.source} value={item.source}>
                  {item.label}
                </option>
              ))}
          </select>
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
