// Presets: saved looks. Click one to apply it; save, rename, copy, export,
// import and delete them here.

import { open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { PageHeader } from "../Shell";
import { useLumen } from "../store";
import type { PresetInfo } from "../types";
import { Button, Dots, Tip, WallpaperArt } from "../ui";

const LUMEN_FILES = [{ name: "Lumen preset", extensions: ["lumen"] }];

/** "just now", "3 hours ago", "2 days ago", or a date for anything older. */
export function lastUsed(seconds: number, now = Date.now()): string {
  if (!seconds) return "Never used";
  const ago = Math.max(0, now / 1000 - seconds);
  if (ago < 90) return "Used just now";
  if (ago < 3600) return `Used ${Math.round(ago / 60)} min ago`;
  if (ago < 86_400) return `Used ${Math.round(ago / 3600)} h ago`;
  if (ago < 14 * 86_400) return `Used ${Math.round(ago / 86_400)} d ago`;
  return `Used ${new Date(seconds * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}`;
}

export function Presets() {
  const { presets, presetRequest, importPresets, shown, go } = useLumen();
  const [saving, setSaving] = useState(false);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  // Any click elsewhere closes an open menu.
  useEffect(() => {
    if (!menuFor) return;
    const close = () => {
      setMenuFor(null);
      setConfirmDelete(null);
    };
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menuFor]);

  const importFromDialog = async () => {
    const picked = await open({ multiple: true, title: "Import presets", filters: LUMEN_FILES });
    if (picked) await importPresets(Array.isArray(picked) ? picked : [picked]);
  };
  const exportToFile = async (preset: PresetInfo) => {
    const path = await save({ title: `Export ${preset.name}`, defaultPath: `${preset.name}.lumen`, filters: LUMEN_FILES });
    if (path) await presetRequest("exportPreset", { id: preset.id, path });
  };

  return (
    <>
      <PageHeader title="Presets" tip="a whole look, saved — one click brings it back">
        <div className="header-actions">
          <Button size="header" className="btn-plain" onClick={() => void importFromDialog()}>
            Import
          </Button>
          <Button size="header" kind="primary" className="btn-plain" disabled={!shown} title={shown ? undefined : "Make a theme first"} onClick={() => setSaving(true)}>
            Save current look
          </Button>
        </div>
      </PageHeader>

      {saving && (
        <NameForm
          label="Name this look"
          initial={shown?.name ?? ""}
          action="Save preset"
          onCancel={() => setSaving(false)}
          onSubmit={async (name) => {
            setSaving(false);
            await presetRequest("savePreset", { name });
          }}
        />
      )}

      <div className="tile-grid preset-grid">
        {presets.map((preset) => (
          <div key={preset.id} className={`tile preset-tile${preset.active ? " preset-active" : ""}`}>
            <button type="button" className="preset-apply" aria-pressed={preset.active} title={`Apply ${preset.name}`} onClick={() => void presetRequest("applyPreset", { id: preset.id })}>
              <WallpaperArt className="tile-art" wallpaper={null} thumb={preset.thumb} colours={preset.palette} />
              <span className="preset-caption">
                {renaming === preset.id ? (
                  <span className="tile-name">&nbsp;</span>
                ) : (
                  <span className="tile-name" title={preset.name}>
                    {preset.name}
                  </span>
                )}
                <span className="muted small">{lastUsed(preset.lastUsedAt)}</span>
                <span className="preset-dots">
                  <Dots colours={preset.palette} size={30} />
                </span>
              </span>
            </button>
            {preset.active && <span className="preset-badge">Active</span>}
            {renaming === preset.id && (
              <RenameBox
                initial={preset.name}
                onDone={(name) => {
                  setRenaming(null);
                  if (name && name !== preset.name) void presetRequest("renamePreset", { id: preset.id, name });
                }}
              />
            )}
            <button
              type="button"
              className="preset-menu-button"
              aria-label={`More actions for ${preset.name}`}
              aria-expanded={menuFor === preset.id}
              onClick={(event) => {
                event.stopPropagation();
                setConfirmDelete(null);
                setMenuFor(menuFor === preset.id ? null : preset.id);
              }}
            >
              ⋯
            </button>
            {menuFor === preset.id && (
              <div className="menu preset-menu" role="menu" onClick={(event) => event.stopPropagation()}>
                <MenuItem
                  onClick={() => {
                    setMenuFor(null);
                    setRenaming(preset.id);
                  }}
                >
                  Rename
                </MenuItem>
                <MenuItem
                  onClick={() => {
                    setMenuFor(null);
                    void presetRequest("duplicatePreset", { id: preset.id });
                  }}
                >
                  Duplicate
                </MenuItem>
                <button type="button" role="menuitem" className="menu-item" disabled title="Schedules arrive in a later version">
                  Schedule
                </button>
                <MenuItem
                  onClick={() => {
                    setMenuFor(null);
                    void exportToFile(preset);
                  }}
                >
                  Export .lumen…
                </MenuItem>
                <button
                  type="button"
                  role="menuitem"
                  className="menu-item menu-item-danger"
                  onClick={() => {
                    if (confirmDelete === preset.id) {
                      setMenuFor(null);
                      setConfirmDelete(null);
                      void presetRequest("deletePreset", { id: preset.id });
                    } else {
                      setConfirmDelete(preset.id);
                    }
                  }}
                >
                  {confirmDelete === preset.id ? "Click again to delete" : "Delete"}
                </button>
              </div>
            )}
          </div>
        ))}

        {presets.length === 0 && (
          <div className="empty">
            <p>No presets yet.</p>
            <p className="muted">
              A preset keeps your theme, each monitor’s wallpaper and where your widgets sit.
              {shown ? " Save the look you have now, or import a .lumen file." : " Make a theme first, then save it here."}
            </p>
            {shown ? (
              <Tip accent size={20}>
                tip: drag a .lumen file in to import it
              </Tip>
            ) : (
              <Button onClick={() => go("wallpapers")}>Pick a wallpaper</Button>
            )}
          </div>
        )}
      </div>
    </>
  );
}

function MenuItem({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" role="menuitem" className="menu-item" onClick={onClick}>
      {children}
    </button>
  );
}

/** A one-line form in a card: name box, action, cancel. */
function NameForm({ label, initial, action, onSubmit, onCancel }: { label: string; initial: string; action: string; onSubmit: (name: string) => void; onCancel: () => void }) {
  const [name, setName] = useState(initial);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.select(), []);
  return (
    <form
      className="card name-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (name.trim()) onSubmit(name.trim());
      }}
    >
      <label className="name-form-field">
        <span className="label">{label}</span>
        <input ref={input} className="text-input" value={name} maxLength={80} autoFocus onChange={(event) => setName(event.target.value)} onKeyDown={(event) => event.key === "Escape" && onCancel()} />
      </label>
      <Button size="header" className="btn-plain" onClick={onCancel}>
        Cancel
      </Button>
      <Button size="header" kind="primary" className="btn-plain" type="submit" disabled={!name.trim()}>
        {action}
      </Button>
    </form>
  );
}

/** The name, editable in place on a tile. Enter or leaving the box applies; Esc cancels. */
function RenameBox({ initial, onDone }: { initial: string; onDone: (name: string | null) => void }) {
  const [name, setName] = useState(initial);
  const finished = useRef(false);
  const finish = (value: string | null) => {
    if (finished.current) return;
    finished.current = true;
    onDone(value);
  };
  return (
    <input
      className="text-input preset-rename"
      value={name}
      maxLength={80}
      autoFocus
      aria-label="Preset name"
      onFocus={(event) => event.target.select()}
      onChange={(event) => setName(event.target.value)}
      onBlur={() => finish(name.trim() || null)}
      onKeyDown={(event) => {
        if (event.key === "Enter") finish(name.trim() || null);
        if (event.key === "Escape") finish(null);
      }}
    />
  );
}
