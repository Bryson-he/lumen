// Widgets: a scaled picture of one monitor's desktop where widgets can be
// selected and dragged, the widget library, and the inspector.

import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { hexToRgb } from "../color";
import { call } from "../core";
import { PageHeader } from "../Shell";
import { useLumen } from "../store";
import {
  WIDGET_CPU_BUDGET,
  WIDGET_GRID,
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

const LIBRARY: { kind: WidgetKind | null; label: string }[] = [
  { kind: "clock", label: "Clock" },
  { kind: "nowPlaying", label: "Now playing" },
  { kind: "systemStats", label: "System stats" },
  { kind: null, label: "Weather" },
  { kind: null, label: "Calendar" },
  { kind: null, label: "Notes" },
  { kind: null, label: "App launcher" },
];
const GLASS_OPACITY = { none: 1, low: 0.9, medium: 0.78, high: 0.64 } as const;
const ZOOM_STEPS = [0.15, 0.2, 0.3, 0.4, 0.5, 0.65, 0.8, 1];

type Tab = "layout" | "style" | "data";

export function WidgetsScreen() {
  const { widgets, widgetRequest, shown, wallpapers, go } = useLumen();
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

      <div className="split widgets-split">
        <div className="widgets-main">
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

          <div className="stack-10">
            <span className="label">Widget library — click to add to this monitor</span>
            <div className="library">
              {LIBRARY.map((item) => (
                <button key={item.label} type="button" className="library-item" disabled={!item.kind} title={item.kind ? `Add a ${item.label} widget` : "Arrives in a later version"} onClick={() => item.kind && add(item.kind)}>
                  {item.label}
                </button>
              ))}
              <button type="button" className="library-item library-gallery" onClick={() => go("gallery")}>
                Browse gallery →
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
              tab={tab}
              onTab={setTab}
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
              <div className="pw-scale" style={{ width: placement.width, height: placement.height, transform: `scale(${scale})`, ...widgetVars(theme, placement.style) }}>
                <WidgetReplica kind={placement.kind} />
              </div>
              {placement.id === selectedId && (
                <>
                  <span className="pw-handle pw-handle-tl" aria-hidden="true" />
                  <span className="pw-handle pw-handle-tr" aria-hidden="true" />
                  <span className="pw-handle pw-handle-bl" aria-hidden="true" />
                  <span className="pw-handle pw-handle-br" aria-hidden="true" />
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

/** One widget's own style choices, layered over the theme. */
function widgetVars(theme: Theme | null, style: WidgetStyle): CSSProperties {
  const vars: Record<string, string | number> = { opacity: style.opacity / 100 };
  if (style.background === "solid") vars["--w-glass-opacity"] = 1;
  if (style.background === "none") {
    vars["--w-glass-opacity"] = 0;
    vars["--w-border-width"] = "0px";
  }
  if (style.radius !== null) vars["--w-radius"] = `${style.radius}px`;
  void theme;
  return vars as CSSProperties;
}

/** A picture of a widget, laid out like the real one (sizes match the desktop renderer). */
function WidgetReplica({ kind }: { kind: WidgetKind }) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    if (kind !== "clock") return;
    const timer = setInterval(() => setNow(new Date()), 20_000);
    return () => clearInterval(timer);
  }, [kind]);

  if (kind === "clock") {
    const date = now.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "short" });
    const parts = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).formatToParts(now);
    const time = parts
      .filter((p) => p.type !== "dayPeriod")
      .map((p) => p.value)
      .join("")
      .trim();
    const suffix = parts.find((p) => p.type === "dayPeriod")?.value ?? "";
    return (
      <div className="wd wd-clock">
        <span className="wd-muted">{date}</span>
        <span className="wd-time">{time}</span>
        {suffix && <span className="wd-suffix">{suffix.toUpperCase()}</span>}
      </div>
    );
  }
  if (kind === "systemStats") {
    return (
      <div className="wd wd-stats">
        <span className="wd-strong">System</span>
        {[
          ["CPU", 18, "var(--w-primary, var(--pencil))"],
          ["RAM", 46, "var(--w-tertiary, var(--success))"],
          ["GPU", 9, "var(--w-accent, var(--pencil))"],
        ].map(([label, value, colour]) => (
          <span key={label} className="wd-row">
            <span className="wd-row-label">
              <span>{label}</span>
              <span className="wd-muted">{value}%</span>
            </span>
            <span className="wd-bar">
              <span style={{ width: `${value}%`, background: colour as string }} />
            </span>
          </span>
        ))}
      </div>
    );
  }
  return (
    <div className="wd wd-media">
      <span className="wd-art" />
      <span className="wd-media-text">
        <span className="wd-title">Track title</span>
        <span className="wd-muted">Artist name</span>
        <span className="wd-bar wd-media-bar">
          <span style={{ width: "42%", background: "var(--w-primary, var(--pencil))" }} />
        </span>
      </span>
    </div>
  );
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
  suspended,
  onUpdate,
  onRemove,
  onResume,
}: {
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
  const { styles } = useLumen();
  const sourceId = useId();
  const monitorSelectId = useId();

  return (
    <>
      <div className="stack-2">
        <span className="small-13 muted">Selected</span>
        <span className="panel-title">{WIDGET_NAMES[placement.kind]}</span>
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
            <label htmlFor={backgroundId} className="small-14">
              Background
            </label>
            <select id={backgroundId} value={style.background} onChange={(event) => setStyle({ background: event.target.value as WidgetBackground })}>
              <option value="glass">Glass (follows theme blur)</option>
              <option value="solid">Solid surface</option>
              <option value="none">None</option>
            </select>
          </div>
          <Slider label="Opacity" min={20} max={100} value={style.opacity} format={(v) => `${v}%`} onCommit={(opacity) => setStyle({ opacity })} />
          <Check checked={style.radius === null} onChange={(follow) => setStyle({ radius: follow ? null : themeRadius })}>
            Corners follow theme
          </Check>
          {style.radius !== null && <Slider label="Corner radius" min={0} max={24} value={style.radius} format={(v) => `${v} px`} onCommit={(radius) => setStyle({ radius })} />}
        </>
      )}

      {tab === "data" && (
        <>
          <div className="field">
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
