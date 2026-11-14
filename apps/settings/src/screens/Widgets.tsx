// Widgets: a scaled picture of one monitor's desktop where widgets can be
// selected and dragged, the widget library, and the inspector.

import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { hexToRgb } from "../color";
import { call } from "../core";
import { PageHeader } from "../Shell";
import { useLumen } from "../store";
import { type Anchor,
  WIDGET_CPU_BUDGET,
  WIDGET_GRID,
  WIDGET_MIN_SIZE,
  WIDGET_MEMORY_BUDGET_MB,
  WIDGET_NAMES,
  type MonitorArea,
  type Placement,
  type Theme,
  type UpdateWidgetParams,
  type WidgetBackground,
  type WidgetCost,
  type WidgetKind,
  type WidgetStyle,
} from "../types";
import { Arrow, Button, Check, Segmented, Slider, Tip, WallpaperArt } from "../ui";
import { ChoiceTiles, lookFor, partsOf, useSampleValues, WidgetView } from "../widgetView";
import { Designer } from "./Designer";
import { WeatherSettings } from "./Weather";
import { PartsTab } from "./Parts";
import { ScriptsWaiting } from "./Scripts";
import { HaPicker } from "./HomeAssistant";

const LIBRARY: { kind: WidgetKind | null; label: string }[] = [
  { kind: "clock", label: "Clock" },
  { kind: "nowPlaying", label: "Now playing" },
  { kind: "systemStats", label: "System stats" },
  { kind: "weather", label: "Weather" },
  { kind: null, label: "Calendar" },
  { kind: null, label: "Notes" },
  { kind: null, label: "App launcher" },
];
const GLASS_OPACITY = { none: 1, low: 0.9, medium: 0.78, high: 0.64 } as const;
const ZOOM_STEPS = [0.15, 0.2, 0.3, 0.4, 0.5, 0.65, 0.8, 1];

type Tab = "layout" | "style" | "parts" | "data";

/** The nine places a widget can be sent to, in reading order. */
const PLACES: { anchor: Anchor; label: string }[] = [
  { anchor: "top-left", label: "Top left" },
  { anchor: "top", label: "Top middle" },
  { anchor: "top-right", label: "Top right" },
  { anchor: "left", label: "Middle left" },
  { anchor: "centre", label: "Centre of the monitor" },
  { anchor: "right", label: "Middle right" },
  { anchor: "bottom-left", label: "Bottom left" },
  { anchor: "bottom", label: "Bottom middle" },
  { anchor: "bottom-right", label: "Bottom right" },
];

export function WidgetsScreen() {
  const { widgets, widgetRequest, shown, wallpapers, styles, partDefaults, widgetFiles, request, go } = useLumen();
  const [newFile, setNewFile] = useState<string | null>(null);
  const [pickingHa, setPickingHa] = useState(false);
  const [managing, setManaging] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  // The designer: one widget large, its parts editable with the mouse.
  const [designing, setDesigning] = useState(false);
  const [partId, setPartId] = useState<string | null>(null);
  const selectedNow = widgets?.selected ?? null;
  useEffect(() => {
    // Another widget (or none): no part of the old one stays selected.
    setPartId(null);
    if (!selectedNow) setDesigning(false);
  }, [selectedNow]);
  const [monitorId, setMonitorId] = useState<string | null>(null);
  const [zoom, setZoom] = useState<number | null>(null);
  const [tab, setTab] = useState<Tab>("layout");
  const [adding, setAdding] = useState(false);

  const monitors = useMemo(() => [...(widgets?.monitors ?? [])].sort((a, b) => a.number - b.number), [widgets?.monitors]);
  const primary = monitors.find((m) => m.primary) ?? monitors[0];
  const monitor = monitors.find((m) => m.id === monitorId) ?? primary;
  const isOn = (placement: Placement, m: MonitorArea) => (placement.monitor ? placement.monitor === m.id : m.primary);

  const selected = widgets?.layout.widgets.find((w) => w.id === widgets.selected) ?? null;
  // Selecting a widget (here or on the desktop) shows the monitor it is on.
  const selectedMonitor = selected && monitors.find((m) => isOn(selected, m))?.id;
  useEffect(() => {
    if (selectedMonitor) setMonitorId(selectedMonitor);
  }, [selectedMonitor, widgets?.selected]);

  if (!widgets || !monitor) {
    return (
      <>
        <PageHeader title="Widgets" tip="drag them around — they recolour with your theme" />
        <section className="card coming-soon">
          <p>The desktop widgets are not running.</p>
          <p className="muted">Lumen could not start them (usually a graphics driver problem). The rest of Lumen still works.</p>
        </section>
      </>
    );
  }

  const { layout, editMode } = widgets;
  const update = (params: UpdateWidgetParams) => void widgetRequest("updateWidget", params);
  const add = (kind: WidgetKind) => {
    setAdding(false);
    void widgetRequest("addWidget", monitor.primary ? { kind } : { kind, monitor: monitor.id });
  };

  return (
    <>
      <PageHeader title="Widgets" tip="drag them around — they recolour with your theme">
        <div className="header-actions header-actions-center">
          <Check checked={layout.snap} onChange={(snap) => void widgetRequest("setLayoutOptions", { snap })}>
            <span className="muted">Snap to grid</span>
          </Check>
          <Button size="header" className="btn-plain" aria-pressed={layout.locked} onClick={() => void widgetRequest("setLayoutOptions", { locked: !layout.locked })}>
            {layout.locked ? "Unlock layout" : "Lock layout"}
          </Button>
          <Button size="header" className="btn-plain" aria-pressed={editMode} onClick={() => void widgetRequest("setEditMode", { on: !editMode })}>
            {editMode ? "Finish editing · Esc" : "Edit on desktop · Win+Alt+E"}
          </Button>
          <div className="menu-anchor">
            <Button size="header" kind="primary" className="btn-plain" aria-expanded={adding} onClick={() => setAdding((open) => !open)}>
              + Add widget
            </Button>
            {adding && (
              <div className="menu" role="menu">
                {LIBRARY.filter((item) => item.kind).map((item) => (
                  <button key={item.label} type="button" role="menuitem" className="menu-item" onClick={() => add(item.kind!)}>
                    {item.label}
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </PageHeader>

      <ScriptsWaiting />
      <div className="split widgets-split">
        <div className="widgets-main">
          {designing && selected && shown && styles && partDefaults ? (
            <Designer
              key={selected.id}
              placement={selected}
              saved={partsOf(selected, partDefaults, widgetFiles, widgets.resolved).parts}
              defaultSize={partsOf(selected, partDefaults, widgetFiles, widgets.resolved).defaultSize}
              file={selected.file ?? null}
              look={lookFor(selected, shown, styles)}
              theme={shown}
              locked={layout.locked}
              pickedId={partId}
              onPick={setPartId}
              onUpdate={update}
              onDone={() => setDesigning(false)}
            />
          ) : (
          <DesktopPreview
            monitor={monitor}
            monitors={monitors}
            placements={layout.widgets.filter((w) => isOn(w, monitor))}
            selectedId={widgets.selected}
            guides={widgets.guides.monitor === monitor.id || (!widgets.guides.monitor && monitor.primary) ? widgets.guides : null}
            locked={layout.locked}
            theme={shown}
            wallpaper={wallpapers.find((w) => monitor.wallpaper && samePath(w.path, monitor.wallpaper)) ?? null}
            zoom={zoom}
            onZoom={setZoom}
            onMonitor={setMonitorId}
            onSelect={(id) => void widgetRequest("selectWidget", { id })}
            onMove={update}
          />
          )}

          <div className="stack-10">
            <span className="label">Widget library — click to add to this monitor</span>
            <div className="library">
              {LIBRARY.map((item) => (
                <button key={item.label} type="button" className="library-item" disabled={!item.kind} title={item.kind ? `Add a ${item.label} widget` : "Arrives in a later version"} onClick={() => item.kind && add(item.kind)}>
                  {item.label}
                </button>
              ))}
              <button type="button" className="library-item" aria-expanded={pickingHa} title="Add a switch or a reading from Home Assistant" onClick={() => setPickingHa((open) => !open)}>
                Home Assistant…
              </button>
              <button type="button" className="library-item library-gallery" onClick={() => go("gallery")}>
                Browse gallery →
              </button>
            </div>
            {pickingHa && (
              <HaPicker
                onClose={() => setPickingHa(false)}
                onPick={async (entity) => {
                  const file = await request<string>("newHaWidget", { entity: entity.id, name: entity.name });
                  if (!file) return;
                  // Give the desktop a moment to notice the new file before placing a widget from it.
                  await new Promise((resolve) => setTimeout(resolve, 500));
                  await widgetRequest("addWidget", monitor.primary ? { kind: "clock", file } : { kind: "clock", file, monitor: monitor.id });
                  setPickingHa(false);
                }}
              />
            )}
          </div>

          <div className="stack-10">
            <span className="label">Your widget files — click to add one; edit them here or in any text editor</span>
            {managing && (
              <div className="strip widget-delete-confirm" role="status">
                {deleting ? (
                  <>
                    <span>
                      Delete <strong>{deleting}</strong>{widgetFiles.find((file) => file.file === deleting)?.script ? " and its script" : ""}? Widgets on your desktop made from it go back to a plain widget. This cannot be undone.
                    </span>
                    <span className="header-actions">
                      <Button size="bar" onClick={() => void request("deleteWidgetFile", { file: deleting }).then(() => setDeleting(null))}>
                        Delete it
                      </Button>
                      <Button size="bar" kind="primary" onClick={() => setDeleting(null)}>
                        Keep it
                      </Button>
                    </span>
                  </>
                ) : (
                  <span>Click a widget file to delete it. Two with the same name are told apart by their file names.</span>
                )}
              </div>
            )}
            <div className="library">
              {widgetFiles
                .filter((file) => file.isWidget)
                .map((file) => (
                  <button
                    key={file.file}
                    type="button"
                    className={`library-item${file.error ? " library-item-broken" : ""}`}
                    title={file.error ? `${file.file} has a mistake${file.errorLine ? ` on line ${file.errorLine}` : ""}: ${file.error}` : `Add a widget from ${file.file}`}
                    onClick={() => (managing ? setDeleting(file.file) : void widgetRequest("addWidget", monitor.primary ? { kind: "clock", file: file.file } : { kind: "clock", file: file.file, monitor: monitor.id }))}
                  >
                    {managing && <span aria-hidden="true">✕ </span>}
                    {file.name}
                    {managing && <span className="muted small"> · {file.file}</span>}
                    {file.error && <span className="muted small"> · mistake</span>}
                  </button>
                ))}
              {widgetFiles.some((file) => file.isWidget) && (
                <button
                  type="button"
                  className="library-item"
                  aria-pressed={managing}
                  title="Delete widget files you no longer want"
                  onClick={() => {
                    setManaging((on) => !on);
                    setDeleting(null);
                  }}
                >
                  {managing ? "Done removing" : "Remove some…"}
                </button>
              )}
              {newFile === null ? (
                <button type="button" className="library-item library-gallery" onClick={() => setNewFile("My widget")}>
                  + New widget file
                </button>
              ) : (
                <span className="parts-naming">
                  <input
                    className="parts-text"
                    type="text"
                    aria-label="Name for the new widget file"
                    maxLength={60}
                    autoFocus
                    value={newFile}
                    onChange={(event) => setNewFile(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && newFile.trim()) void request<string>("newWidgetFile", { name: newFile.trim() }).then((made) => made && setNewFile(null));
                      if (event.key === "Escape") setNewFile(null);
                    }}
                  />
                  <Button size="bar" kind="primary" disabled={!newFile.trim()} onClick={() => void request<string>("newWidgetFile", { name: newFile.trim() }).then((made) => made && setNewFile(null))}>
                    Create
                  </Button>
                  <Button size="bar" onClick={() => setNewFile(null)}>
                    Cancel
                  </Button>
                </span>
              )}
              <button
                type="button"
                className="library-item"
                title="Add a widget someone shared as a .lumenwidget file"
                onClick={() =>
                  void open({ multiple: true, title: "Import widgets", filters: [{ name: "Lumen widget", extensions: ["lumenwidget"] }] }).then(async (chosen) => {
                    for (const path of chosen ? (Array.isArray(chosen) ? chosen : [chosen]) : []) await request<string>("importWidgetPackage", { path });
                  })
                }
              >
                Import a shared widget…
              </button>
              <button type="button" className="library-item" onClick={() => void request("openWidgetFolder")}>
                Open the folder
              </button>
            </div>
          </div>
        </div>

        <aside className="panel inspector" aria-label="Inspector">
          {selected ? (
            <Inspector
              key={selected.id}
              placement={selected}
              monitors={monitors}
              currentMonitor={monitors.find((m) => isOn(selected, m)) ?? monitor}
              themeRadius={shown?.style.radius ?? 12}
              locked={layout.locked}
              tab={designing ? "parts" : tab}
              onTab={(next) => {
                setTab(next);
                if (next !== "parts") setDesigning(false);
              }}
              partId={partId}
              onPart={setPartId}
              designing={designing}
              onDesign={() => {
                setTab("parts");
                setDesigning(true);
              }}
              suspended={widgets.suspended.includes(selected.id)}
              onUpdate={update}
              onRemove={() => void widgetRequest("removeWidget", { id: selected.id })}
              onResume={() => void widgetRequest("resumeWidget", { id: selected.id })}
            />
          ) : (
            <>
              <div className="stack-2">
                <span className="small-13 muted">Selected</span>
                <span className="panel-title">Nothing yet</span>
              </div>
              <p className="muted small-14">Click a widget in the picture to move it or change how it looks.</p>
              <Tip accent size={20}>
                or press Win+Alt+E and drag them on the real desktop
              </Tip>
            </>
          )}
        </aside>
      </div>
    </>
  );
}

function samePath(a: string, b: string) {
  const tidy = (p: string) => p.replace(/\\/g, "/").toLowerCase();
  return tidy(a) === tidy(b);
}

// ---------- The desktop picture ----------

function DesktopPreview({
  monitor,
  monitors,
  placements,
  selectedId,
  guides,
  locked,
  theme,
  wallpaper,
  zoom,
  onZoom,
  onMonitor,
  onSelect,
  onMove,
}: {
  monitor: MonitorArea;
  monitors: MonitorArea[];
  placements: Placement[];
  selectedId: string | null;
  guides: { x: number | null; y: number | null } | null;
  locked: boolean;
  theme: Theme | null;
  wallpaper: ReturnType<typeof useLumen>["wallpapers"][number] | null;
  zoom: number | null;
  onZoom: (zoom: number | null) => void;
  onMonitor: (id: string) => void;
  onSelect: (id: string | null) => void;
  onMove: (params: UpdateWidgetParams) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [boxWidth, setBoxWidth] = useState(800);
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setBoxWidth(element.clientWidth));
    observer.observe(element);
    setBoxWidth(element.clientWidth);
    return () => observer.disconnect();
  }, []);

  // Fit the monitor to the box, but never so small that widgets cannot be grabbed:
  // very wide monitors scroll sideways instead.
  const fit = boxWidth / monitor.width;
  const scale = zoom ?? (fit >= 0.3 ? Math.min(fit, 1) : 0.5);
  const step = (direction: 1 | -1) => {
    const next = direction > 0 ? ZOOM_STEPS.find((z) => z > scale + 0.001) : [...ZOOM_STEPS].reverse().find((z) => z < scale - 0.001);
    if (next) onZoom(next);
  };

  // Dragging: remember where the widget and the pointer started, ask the core
  // for each new position, and draw whatever (snapped) position comes back.
  const drag = useRef<{ id: string; startX: number; startY: number; originX: number; originY: number; last: { x: number; y: number } | null; frame: number } | null>(null);
  const begin = (event: ReactPointerEvent<HTMLDivElement>, placement: Placement) => {
    event.stopPropagation();
    onSelect(placement.id);
    if (locked || event.button !== 0) return;
    try {
      // Keeps the drag alive when the pointer leaves the widget.
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Not every pointer can be captured; the drag still works while over the widget.
    }
    drag.current = { id: placement.id, startX: event.clientX, startY: event.clientY, originX: placement.x, originY: placement.y, last: null, frame: 0 };
  };
  const move = (event: ReactPointerEvent<HTMLDivElement>) => {
    const state = drag.current;
    if (!state) return;
    const x = state.originX + (event.clientX - state.startX) / scale;
    const y = state.originY + (event.clientY - state.startY) / scale;
    // Ignore the tiny wobble of a plain click.
    if (!state.last && Math.hypot(event.clientX - state.startX, event.clientY - state.startY) < 3) return;
    state.last = { x, y };
    if (!state.frame) {
      state.frame = requestAnimationFrame(() => {
        state.frame = 0;
        if (drag.current === state && state.last) onMove({ id: state.id, ...state.last, dragging: true });
      });
    }
  };
  const end = () => {
    const state = drag.current;
    drag.current = null;
    if (!state) return;
    cancelAnimationFrame(state.frame);
    if (state.last) onMove({ id: state.id, ...state.last });
  };

  // Resizing by a corner: the opposite corner stays where it is. Sizes move in
  // grid steps and never go below the smallest widget.
  const resize = useRef<{ id: string; right: boolean; bottom: boolean; startX: number; startY: number; origin: Placement; last: UpdateWidgetParams | null; frame: number } | null>(null);
  const beginResize = (event: ReactPointerEvent<HTMLSpanElement>, placement: Placement, right: boolean, bottom: boolean) => {
    event.stopPropagation();
    if (locked || event.button !== 0) return;
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // As for moving: the drag still works while the pointer is over the handle.
    }
    resize.current = { id: placement.id, right, bottom, startX: event.clientX, startY: event.clientY, origin: placement, last: null, frame: 0 };
  };
  const moveResize = (event: ReactPointerEvent<HTMLSpanElement>) => {
    const state = resize.current;
    if (!state) return;
    event.stopPropagation();
    const { origin } = state;
    const dx = (event.clientX - state.startX) / scale;
    const dy = (event.clientY - state.startY) / scale;
    const stepped = (value: number) => Math.round(value / WIDGET_GRID) * WIDGET_GRID;
    const width = Math.max(WIDGET_MIN_SIZE[0], stepped(origin.width + (state.right ? dx : -dx)));
    const height = Math.max(WIDGET_MIN_SIZE[1], stepped(origin.height + (state.bottom ? dy : -dy)));
    const next: UpdateWidgetParams = { id: state.id, width, height };
    // Dragging a left or top corner moves that edge, so the far corner stays put.
    if (!state.right || !state.bottom) {
      next.x = state.right ? origin.x : Math.max(0, origin.x + origin.width - width);
      next.y = state.bottom ? origin.y : Math.max(0, origin.y + origin.height - height);
    }
    state.last = next;
    if (!state.frame) {
      state.frame = requestAnimationFrame(() => {
        state.frame = 0;
        if (resize.current === state && state.last) onMove({ ...state.last, dragging: true });
      });
    }
  };
  const endResize = (event: ReactPointerEvent<HTMLSpanElement>) => {
    const state = resize.current;
    resize.current = null;
    if (!state) return;
    event.stopPropagation();
    cancelAnimationFrame(state.frame);
    if (state.last) onMove(state.last);
  };
  const handle = (placement: Placement, corner: "tl" | "tr" | "bl" | "br") => (
    <span
      className={`pw-handle pw-handle-${corner}`}
      aria-hidden="true"
      onPointerDown={(event) => beginResize(event, placement, corner.endsWith("r"), corner.startsWith("b"))}
      onPointerMove={moveResize}
      onPointerUp={endResize}
      onPointerCancel={endResize}
    />
  );

  // Bring the selected widget into view when the selection changes (it may be
  // far along a wide monitor, or have been picked on the real desktop).
  useEffect(() => {
    box.current?.querySelector(".pw-selected")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedId, monitor.id]);

  const index = monitors.findIndex((m) => m.id === monitor.id);
  const canvasHeight = monitor.height * scale;
  const scrollsSideways = monitor.width * scale > boxWidth + 1;
  const visibleHeight = Math.min(Math.max(canvasHeight, 240), 470);
  // With no library image to show, a soft backdrop in the theme's colours.
  const backdrop = theme ? [theme.roles.surface, theme.roles.neutral, theme.roles.primary] : undefined;
  const style = { "--preview-grid-size": `${WIDGET_GRID * scale}px`, "--pw-scale": scale, width: monitor.width * scale, height: canvasHeight, ...themeVars(theme) } as CSSProperties;

  return (
    <div className="preview-frame">
      <div ref={box} className="preview-box" style={{ height: visibleHeight + 2 + (scrollsSideways ? 16 : 0), overflowY: canvasHeight <= visibleHeight ? "hidden" : "auto" }}>
        <div
          className="preview-canvas"
          style={style}
          role="group"
          aria-label={`Desktop layout for Monitor ${monitor.number} with ${placements.length} widgets`}
          onPointerDown={() => onSelect(null)}
        >
          <WallpaperArt className="preview-wallpaper" wallpaper={wallpaper} colours={backdrop} />
          <div className="preview-grid" aria-hidden="true" />
          {guides?.x != null && <div className="guide guide-x" aria-hidden="true" style={{ left: guides.x * scale }} />}
          {guides?.y != null && <div className="guide guide-y" aria-hidden="true" style={{ top: guides.y * scale }} />}
          {placements.map((placement) => (
            <div
              key={placement.id}
              className={`pw${placement.id === selectedId ? " pw-selected" : ""}${locked ? " pw-locked" : ""}`}
              style={{ left: placement.x * scale, top: placement.y * scale, width: placement.width * scale, height: placement.height * scale }}
              role="button"
              tabIndex={0}
              aria-pressed={placement.id === selectedId}
              aria-label={`${WIDGET_NAMES[placement.kind]} at ${Math.round(placement.x)}, ${Math.round(placement.y)}`}
              onPointerDown={(event) => begin(event, placement)}
              onPointerMove={move}
              onPointerUp={end}
              onPointerCancel={end}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") onSelect(placement.id);
                // Arrow keys nudge by one grid step.
                const nudge = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[event.key];
                if (nudge && !locked) {
                  event.preventDefault();
                  onMove({ id: placement.id, x: placement.x + nudge[0] * WIDGET_GRID, y: placement.y + nudge[1] * WIDGET_GRID });
                }
              }}
            >
              <div className="pw-scale" style={{ width: placement.width, height: placement.height, transform: `scale(${scale})` }}>
                <PreviewWidget placement={placement} theme={theme} />
              </div>
              {placement.id === selectedId && (
                <>
                  {!locked && (
                    <>
                      {handle(placement, "tl")}
                      {handle(placement, "tr")}
                      {handle(placement, "bl")}
                      {handle(placement, "br")}
                    </>
                  )}
                  <span className={placement.y * scale < 40 ? "pw-badge pw-badge-below" : "pw-badge"} aria-hidden="true">
                    {Math.round(placement.width)} × {Math.round(placement.height)}
                  </span>
                </>
              )}
            </div>
          ))}
          {placements.length === 0 && (
            <div className="preview-empty">
              <Tip size={22}>no widgets on this monitor yet</Tip>
              <Arrow />
            </div>
          )}
        </div>
      </div>

      <div className="preview-footer">
        {monitors.length > 1 ? (
          <button type="button" className="preview-chip" onClick={() => onMonitor(monitors[(index + 1) % monitors.length].id)} title="Show the next monitor">
            Monitor {monitor.number} · {index + 1} of {monitors.length}
          </button>
        ) : (
          <span className="preview-chip">Monitor {monitor.number}</span>
        )}
        <span className="preview-zoom">
          <button type="button" className="preview-chip" aria-label="Zoom out" disabled={scale <= ZOOM_STEPS[0] + 0.001} onClick={() => step(-1)}>
            −
          </button>
          <button type="button" className="preview-chip" onClick={() => onZoom(null)} title="Fit to the window">
            {Math.round(scale * 100)}%
          </button>
          <button type="button" className="preview-chip" aria-label="Zoom in" disabled={scale >= 1} onClick={() => step(1)}>
            +
          </button>
        </span>
      </div>
    </div>
  );
}

/** The theme, as CSS variables for the widget pictures. Without a theme the app's own tokens apply. */
function themeVars(theme: Theme | null): CSSProperties {
  if (!theme) return {};
  const r = theme.roles;
  const text = hexToRgb(r.onSurface) ?? [236, 236, 234];
  const outline = hexToRgb(r.outline) ?? [128, 128, 128];
  return {
    "--w-surface": r.surface,
    "--w-surface-rgb": (hexToRgb(r.surface) ?? [28, 29, 33]).join(","),
    "--w-glass-opacity": GLASS_OPACITY[theme.style.blur],
    "--w-text": r.onSurface,
    "--w-text-muted": `rgba(${text.join(",")},0.72)`,
    "--w-border": `rgba(${text.join(",")},0.14)`,
    "--w-border-width": `${theme.style.border}px`,
    "--w-track": `rgba(${outline.join(",")},0.55)`,
    "--w-primary": r.primary,
    "--w-tertiary": r.tertiary,
    "--w-accent": r.accent,
    "--w-radius": `${theme.style.radius}px`,
    "--w-font": `'${theme.style.font}', var(--font-body)`,
  } as CSSProperties;
}

/** A widget in the preview, drawn from its real parts and component style. */
function PreviewWidget({ placement, theme }: { placement: Placement; theme: Theme | null }) {
  const { styles, partDefaults, widgetFiles, widgets } = useLumen();
  const values = useSampleValues();
  if (!theme || !styles || !partDefaults) return null;
  const { parts, defaultSize } = partsOf(placement, partDefaults, widgetFiles, widgets?.resolved);
  return <WidgetView parts={parts} look={lookFor(placement, theme, styles)} size={[placement.width, placement.height]} defaultSize={defaultSize} values={values} zoom={placement.scale ?? 1} />;
}

// ---------- The inspector ----------

function Inspector({
  placement,
  monitors,
  currentMonitor,
  themeRadius,
  locked,
  tab,
  onTab,
  partId,
  onPart,
  designing,
  onDesign,
  suspended,
  onUpdate,
  onRemove,
  onResume,
}: {
  partId: string | null;
  onPart: (id: string | null) => void;
  designing: boolean;
  onDesign: () => void;
  placement: Placement;
  monitors: MonitorArea[];
  currentMonitor: MonitorArea;
  themeRadius: number;
  locked: boolean;
  tab: Tab;
  onTab: (tab: Tab) => void;
  /** The governor switched this widget off. */
  suspended: boolean;
  onUpdate: (params: UpdateWidgetParams) => void;
  onRemove: () => void;
  onResume: () => void;
}) {
  const { id, style } = placement;
  const setStyle = (changes: Partial<WidgetStyle>) => onUpdate({ id, style: { ...style, ...changes } });
  const backgroundId = useId();
  const componentStyleId = useId();
  const { styles, widgetFiles: files, shown } = useLumen();
  const sourceId = useId();
  const monitorSelectId = useId();

  return (
    <>
      <div className="stack-2">
        <span className="small-13 muted">Selected</span>
        <span className="panel-title">{placement.file ? (files.find((file) => file.file === placement.file)?.name ?? placement.file) : WIDGET_NAMES[placement.kind]}</span>
      </div>
      {suspended && (
        <div className="strip strip-danger suspended-note" role="alert">
          <span>Switched off: it was using too much CPU.</span>
          <button type="button" className="link" onClick={onResume}>
            Resume
          </button>
        </div>
      )}
      <Segmented
        label="Inspector section"
        look="inset"
        value={tab}
        onChange={onTab}
        options={[
          { value: "layout", label: "Layout" },
          { value: "style", label: "Style" },
          { value: "parts", label: "Parts" },
          { value: "data", label: "Data" },
        ]}
      />

      {tab === "layout" && (
        <>
          <div className="field-grid">
            <NumberField label="Width" value={placement.width} min={96} max={2000} disabled={locked} onCommit={(width) => onUpdate({ id, width })} />
            <NumberField label="Height" value={placement.height} min={56} max={2000} disabled={locked} onCommit={(height) => onUpdate({ id, height })} />
            <NumberField label="X" value={placement.x} min={0} max={20000} disabled={locked} onCommit={(x) => onUpdate({ id, x })} />
            <NumberField label="Y" value={placement.y} min={0} max={20000} disabled={locked} onCommit={(y) => onUpdate({ id, y })} />
          </div>
          <Slider
            label="Contents size"
            min={40}
            max={500}
            value={Math.round((placement.scale ?? 1) * 100)}
            format={(v) => `${v}%`}
            disabled={locked}
            onCommit={(percent) => {
              // The box grows with its contents, so nothing is cut off.
              const ratio = percent / 100 / (placement.scale ?? 1);
              onUpdate({ id, scale: percent / 100, width: Math.round(placement.width * ratio), height: Math.round(placement.height * ratio) });
            }}
          />
          <div className="field">
            <span className="small-14" id={`${monitorSelectId}-place`}>
              Put it at
            </span>
            <div className="place-grid" role="group" aria-labelledby={`${monitorSelectId}-place`}>
              {PLACES.map((place) => (
                <button key={place.anchor} type="button" className="place-spot" title={place.label} aria-label={place.label} disabled={locked} onClick={() => onUpdate({ id, anchor: place.anchor })}>
                  <span aria-hidden="true" />
                </button>
              ))}
            </div>
            <span className="muted small">A corner, an edge or the middle of its monitor. Dragging also snaps to the middle.</span>
          </div>
          {monitors.length > 1 && (
            <div className="field">
              <label htmlFor={monitorSelectId} className="small-14">
                Monitor
              </label>
              <select id={monitorSelectId} value={currentMonitor.id} disabled={locked} onChange={(event) => onUpdate({ id, monitor: event.target.value })}>
                {monitors.map((m) => (
                  <option key={m.id} value={m.id}>
                    Monitor {m.number}
                    {m.primary ? " (main)" : ""}
                  </option>
                ))}
              </select>
            </div>
          )}
          {locked && <p className="muted small-13">The layout is locked. Unlock it to move or resize widgets.</p>}
        </>
      )}

      {tab === "style" && (
        <>
          {styles && (
            <div className="field">
              <label htmlFor={componentStyleId} className="small-14">
                Style
              </label>
              <select id={componentStyleId} value={placement.componentStyle ?? ""} onChange={(event) => onUpdate({ id, componentStyle: event.target.value })}>
                <option value="">Same as all widgets</option>
                {[...styles.builtin, ...styles.styles].map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
              <span className="muted small-13">Make your own under Theme &amp; colours → Component styles.</span>
            </div>
          )}
          <div className="field">
            <span className="small-14">Background</span>
            {shown && styles ? (
              <ChoiceTiles<WidgetBackground>
                label="Background"
                value={style.background}
                onChange={(background) => setStyle({ background })}
                options={[
                  { value: "glass", label: "As its style", look: lookFor({ ...placement, style: { ...style, background: "glass", opacity: 100 } }, shown, styles) },
                  { value: "solid", label: "Solid", look: lookFor({ ...placement, style: { ...style, background: "solid", opacity: 100 } }, shown, styles) },
                  { value: "none", label: "None", look: lookFor({ ...placement, style: { ...style, background: "none", opacity: 100 } }, shown, styles) },
                ]}
              />
            ) : (
              <select id={backgroundId} aria-label="Background" value={style.background} onChange={(event) => setStyle({ background: event.target.value as WidgetBackground })}>
                <option value="glass">As its style</option>
                <option value="solid">Solid surface</option>
                <option value="none">None</option>
              </select>
            )}
          </div>
          <Slider label="Opacity" min={20} max={100} value={style.opacity} format={(v) => `${v}%`} onCommit={(opacity) => setStyle({ opacity })} />
          <Check checked={style.radius === null} onChange={(follow) => setStyle({ radius: follow ? null : themeRadius })}>
            Corners follow theme
          </Check>
          {style.radius !== null && <Slider label="Corner radius" min={0} max={24} value={style.radius} format={(v) => `${v} px`} onCommit={(radius) => setStyle({ radius })} />}
        </>
      )}

      {tab === "parts" && <PartsTab placement={placement} locked={locked} openId={partId} designing={designing} onOpen={onPart} onDesign={onDesign} onUpdate={onUpdate} />}

      {tab === "data" && placement.kind === "weather" && <WeatherSettings />}
      {tab === "data" && (
        <>
          <div className="field" hidden={placement.kind === "weather"}>
            <label htmlFor={sourceId} className="small-14">
              Data source
            </label>
            <select id={sourceId} value="builtin" disabled>
              <option value="builtin">{placement.kind === "nowPlaying" ? "Windows media session" : "Built into Lumen"}</option>
            </select>
            <span className="muted small-13">More sources arrive with plugins.</span>
          </div>
          <Check checked={style.showOnHover} onChange={(showOnHover) => setStyle({ showOnHover })}>
            Only show on hover
          </Check>
          <Check checked={style.hideInGames} onChange={(hideInGames) => setStyle({ hideInGames })}>
            Hide when a game is fullscreen
          </Check>
          <CostLine id={id} />
        </>
      )}

      <Button kind="danger" size="header" className="btn-plain" onClick={onRemove}>
        Remove widget
      </Button>
    </>
  );
}

/**
 * What this widget costs, as the core's governor last measured it. Asked for
 * every few seconds, and only while this is on screen and no game is running.
 */
function CostLine({ id }: { id: string }) {
  const { status } = useLumen();
  const [cost, setCost] = useState<WidgetCost | null>(null);
  const gameRunning = status?.gameMode.active ?? false;
  useEffect(() => {
    if (gameRunning) return;
    let cancelled = false;
    const refresh = () => {
      if (document.visibilityState !== "visible") return;
      call<WidgetCost[]>("getWidgetCosts").then(
        (costs) => !cancelled && setCost(costs.find((c) => c.id === id) ?? null),
        () => {},
      );
    };
    refresh();
    const timer = setInterval(refresh, 5000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [id, gameRunning]);

  return (
    <div className="field">
      <span className="small-14">Cost</span>
      {cost ? (
        <span className={cost.status === "ok" ? "muted small-13 cost" : "small-13 cost cost-over"}>
          CPU {cost.cpuPercent.toFixed(3)}% · Memory {cost.memoryMb.toFixed(1)} MB · {Math.round(cost.drawsPerMinute)} redraws a minute
          {cost.status === "overBudget" && ` — over its budget (${WIDGET_CPU_BUDGET}% CPU, ${WIDGET_MEMORY_BUDGET_MB} MB)`}
        </span>
      ) : (
        <span className="muted small-13">Measuring…</span>
      )}
    </div>
  );
}

/** A number box that applies its value on Enter or when focus leaves it. */
function NumberField({ label, value, min, max, disabled, onCommit }: { label: string; value: number; min: number; max: number; disabled?: boolean; onCommit: (value: number) => void }) {
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
