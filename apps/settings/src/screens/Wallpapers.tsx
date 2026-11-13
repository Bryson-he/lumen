// Wallpapers: the library, with each image's extracted palette.

import { useEffect, useMemo, useState } from "react";
import { colourMatch } from "../color";
import { PageHeader, useImportDialog } from "../Shell";
import { useLumen } from "../store";
import type { WallpaperEntry } from "../types";
import { Button, Check, Dots, SearchIcon, Segmented, Tip, WallpaperArt } from "../ui";

type Filter = "all" | "static" | "animated" | "favourites" | `tag:${string}`;

export function Wallpapers() {
  const { wallpapers, monitors, shown, wallpaperOf, previewTheme, setWallpaperOnly, updateWallpaper, go } = useLumen();
  const importFromDialog = useImportDialog();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showOn, setShowOn] = useState<string>("all");
  const [addingTag, setAddingTag] = useState(false);

  const tags = useMemo(() => [...new Set(wallpapers.flatMap((w) => w.tags))].sort(), [wallpapers]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return wallpapers.filter((w) => {
      if (filter === "static" && w.kind !== "static") return false;
      if (filter === "animated" && w.kind !== "animated") return false;
      if (filter === "favourites" && !w.favourite) return false;
      if (filter.startsWith("tag:") && !w.tags.includes(filter.slice(4))) return false;
      if (!q) return true;
      const byColour = colourMatch(q, w.palette);
      if (byColour !== null) return byColour;
      return w.name.toLowerCase().includes(q) || w.tags.some((t) => t.includes(q));
    });
  }, [wallpapers, query, filter]);

  // Keep something selected: the chosen one, else the active wallpaper, else the first shown.
  const selected = wallpapers.find((w) => w.id === selectedId) ?? wallpaperOf(shown) ?? visible[0] ?? null;
  useEffect(() => {
    if (filter.startsWith("tag:") && !tags.includes(filter.slice(4))) setFilter("all");
  }, [tags, filter]);

  const monitorOptions = [
    ...monitors.map((m) => ({ value: m.id, label: `Monitor ${m.number}` })),
    { value: "all", label: monitors.length === 2 ? "Both" : "All" },
  ];
  const target = showOn === "all" || !monitors.some((m) => m.id === showOn) ? undefined : showOn;

  return (
    <>
      <PageHeader title="Wallpapers" tip="each one keeps its own palette, ready to go">
        <div className="header-actions">
          <label className="search">
            <SearchIcon />
            <input value={query} onChange={(event) => setQuery(event.target.value)} aria-label="Search wallpapers" placeholder="Search or filter by colour" />
          </label>
          <Button
            kind="primary"
            size="header"
            onClick={async () => {
              const entry = await importFromDialog();
              if (entry) setSelectedId(entry.id);
            }}
          >
            + Import
          </Button>
        </div>
      </PageHeader>

      <div role="group" aria-label="Filter" className="pills">
        {(
          [
            ["all", "All"],
            ["static", "Static"],
            ["animated", "Animated"],
            ["favourites", "Favourites"],
            ...tags.map((t) => [`tag:${t}`, t] as [Filter, string]),
          ] as [Filter, string][]
        ).map(([value, label]) => (
          <button key={value} type="button" className="pill" aria-pressed={filter === value} onClick={() => setFilter(value)}>
            {label}
          </button>
        ))}
        {addingTag && selected ? (
          <TagInput
            onDone={(tag) => {
              setAddingTag(false);
              if (tag) void updateWallpaper(selected.id, { tags: [...selected.tags, tag] });
            }}
          />
        ) : (
          <button type="button" className="pill pill-dashed" disabled={!selected} title={selected ? `Tag ${selected.name}` : undefined} onClick={() => setAddingTag(true)}>
            + Tag
          </button>
        )}
      </div>

      <div className="split">
        <div className="tile-grid">
          {visible.map((w) => (
            <button key={w.id} type="button" className="tile" aria-pressed={selected?.id === w.id} onClick={() => setSelectedId(w.id)}>
              <WallpaperArt className="tile-art" wallpaper={w} />
              <span className="tile-caption">
                <span className="tile-name">{w.name}</span>
                <span className="muted small">{w.kind === "animated" ? "Animated" : "Static"}</span>
              </span>
            </button>
          ))}
          {visible.length === 0 && (
            <div className="empty">
              {wallpapers.length === 0 ? (
                <>
                  <p>Your library is empty.</p>
                  <Tip accent size={20}>
                    drag any image in, or use + Import
                  </Tip>
                </>
              ) : filter === "animated" ? (
                <p className="muted">No animated wallpapers yet. They arrive with the wallpaper player.</p>
              ) : (
                <p className="muted">Nothing matches. Try another word, a colour like “blue”, or a different filter.</p>
              )}
            </div>
          )}
        </div>

        {selected && (
          <SidePanel
            wallpaper={selected}
            showOn={target ?? "all"}
            monitorOptions={monitors.length > 1 ? monitorOptions : []}
            onShowOn={setShowOn}
            onFavourite={(favourite) => void updateWallpaper(selected.id, { favourite })}
            onRemoveTag={(tag) => void updateWallpaper(selected.id, { tags: selected.tags.filter((t) => t !== tag) })}
            onMakeTheme={() => {
              go("theme");
              void previewTheme({ source: { type: "wallpaper", path: selected.file }, ...(target ? { wallpaperMonitor: target } : {}) });
            }}
            onWallpaperOnly={() => void setWallpaperOnly(selected.file, target)}
          />
        )}
      </div>
    </>
  );
}

function SidePanel({
  wallpaper,
  showOn,
  monitorOptions,
  onShowOn,
  onFavourite,
  onRemoveTag,
  onMakeTheme,
  onWallpaperOnly,
}: {
  wallpaper: WallpaperEntry;
  showOn: string;
  monitorOptions: { value: string; label: string }[];
  onShowOn: (value: string) => void;
  onFavourite: (favourite: boolean) => void;
  onRemoveTag: (tag: string) => void;
  onMakeTheme: () => void;
  onWallpaperOnly: () => void;
}) {
  return (
    <aside className="panel side-panel" aria-label="Selected wallpaper">
      <WallpaperArt className="side-art" wallpaper={wallpaper} label={`Preview of ${wallpaper.name}`} />
      <div className="stack-2">
        <span className="panel-title">{wallpaper.name}</span>
        <span className="muted small-13">{wallpaper.kind === "animated" ? "Animated · pauses automatically in games" : "Static image · no CPU cost"}</span>
      </div>
      <div className="stack-8">
        <span className="label">Extracted palette</span>
        <div className="dot-row">
          <Dots colours={wallpaper.palette.slice(0, 4)} size={36} />
        </div>
      </div>
      {monitorOptions.length > 0 && (
        <div className="stack-8">
          <span className="label">Show on</span>
          <Segmented label="Monitor" look="grid-3" options={monitorOptions} value={showOn} onChange={onShowOn} />
        </div>
      )}
      <Check checked={wallpaper.favourite} onChange={onFavourite}>
        Favourite
      </Check>
      {wallpaper.tags.length > 0 && (
        <div className="tag-row">
          {wallpaper.tags.map((tag) => (
            <button key={tag} type="button" className="tag" title={`Remove the tag ${tag}`} onClick={() => onRemoveTag(tag)}>
              {tag} <span aria-hidden="true">×</span>
            </button>
          ))}
        </div>
      )}
      <Button kind="primary" onClick={onMakeTheme}>
        Make theme from this
      </Button>
      <Button onClick={onWallpaperOnly}>Set as wallpaper only</Button>
      <Tip accent size={20}>
        tip: drag any image in to add it
      </Tip>
    </aside>
  );
}

function TagInput({ onDone }: { onDone: (tag: string | null) => void }) {
  const [value, setValue] = useState("");
  return (
    <input
      autoFocus
      className="pill pill-input"
      value={value}
      maxLength={24}
      aria-label="New tag"
      placeholder="tag name"
      onChange={(event) => setValue(event.target.value)}
      onBlur={() => onDone(value.trim() || null)}
      onKeyDown={(event) => {
        if (event.key === "Enter") onDone(value.trim() || null);
        if (event.key === "Escape") onDone(null);
      }}
    />
  );
}
