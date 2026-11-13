// The widget designer: one widget drawn large, where its parts can be
// selected, dragged, resized, added and removed with the mouse. Every change
// goes to the real widget on the desktop as it is made.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import type { Part, Placement, Theme, UpdateWidgetParams } from "../types";
import { Button } from "../ui";
import { useLumen } from "../store";
import { CodeView } from "./CodeView";
import { allSources, PART_KIND_NAMES, rectIn, sampleValues, WidgetView, type Look } from "../widgetView";

/** How close (in screen pixels) an edge must come to a line to snap to it. */
const SNAP_REACH = 6;
const MIN_PART = 4;
const HISTORY_LIMIT = 60;

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
type Guides = { x: number | null; y: number | null };

/** Snaps one axis: the start of a box of `size` to the nearest of `lines`, by its start, middle or end. */
function snapAxis(start: number, size: number, lines: number[], reach: number): { start: number; line: number | null } {
  let best: { start: number; line: number; distance: number } | null = null;
  for (const line of lines) {
    for (const offset of [0, size / 2, size]) {
      const distance = Math.abs(start + offset - line);
      if (distance <= reach && (!best || distance < best.distance)) best = { start: line - offset, line, distance };
    }
  }
  return best ? { start: best.start, line: best.line } : { start, line: null };
}

export function Designer({
  placement,
  saved,
  defaultSize,
  file,
  look,
  theme,
  locked,
  pickedId,
  onPick,
  onUpdate,
  onDone,
}: {
  placement: Placement;
  /** The widget's parts as saved, and the widget size they are laid out for. */
  saved: Part[];
  defaultSize: readonly [number, number];
  /** The widget file it is drawn from, if any: changes are written to it. */
  file: string | null;
  look: Look;
  theme: Theme;
  locked: boolean;
  pickedId: string | null;
  onPick: (id: string | null) => void;
  onUpdate: (params: UpdateWidgetParams) => void;
  onDone: () => void;
}) {
  const size = [placement.width, placement.height] as const;
  const defaults = { size: defaultSize };
  const [code, setCode] = useState(false);
  // While dragging, the canvas shows this instead of waiting for the core to answer.
  const [draft, setDraft] = useState<Part[] | null>(null);
  const parts = draft ?? saved;
  const [guides, setGuides] = useState<Guides>({ x: null, y: null });
  const [adding, setAdding] = useState(false);
  // Undo: the arrangements before each change made here.
  const [past, setPast] = useState<Part[][]>([]);
  const [future, setFuture] = useState<Part[][]>([]);
  useEffect(() => setDraft(null), [saved]);

  const box = useRef<HTMLDivElement>(null);
  const [boxWidth, setBoxWidth] = useState(700);
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setBoxWidth(element.clientWidth));
    observer.observe(element);
    setBoxWidth(element.clientWidth);
    return () => observer.disconnect();
  }, []);
  // As large as fits, in steps of a quarter so text stays crisp.
  const zoom = Math.max(1, Math.min(4, Math.floor(Math.min((boxWidth - 96) / size[0], 380 / size[1]) * 4) / 4));

  const values = useMemo(() => sampleValues(new Date()), []);
  const picked = parts.find((part) => part.id === pickedId) ?? null;
  const shown = (part: Part): Rect => rectIn(part, size, defaults.size);
  /** Turns a rectangle as drawn back into what the part stores (see `rectIn`). */
  const stored = (part: Part, rect: Rect): Pick<Part, "x" | "y" | "width" | "height"> => ({
    x: Math.round(rect.x),
    y: Math.round(part.fromBottom ? rect.y - (size[1] - defaults.size[1]) : rect.y),
    width: Math.max(1, Math.round(part.stretch ? rect.width - (size[0] - defaults.size[0]) : rect.width)),
    height: Math.max(1, Math.round(rect.height)),
  });

  /** Makes a change final: remembers the old arrangement and saves the new one. */
  const commit = (next: Part[]) => {
    setPast((list) => [...list.slice(-HISTORY_LIMIT + 1), saved]);
    setFuture([]);
    setDraft(next);
    onUpdate({ id: placement.id, parts: next });
  };
  const undo = () => {
    const previous = past[past.length - 1];
    if (!previous) return;
    setPast(past.slice(0, -1));
    setFuture([saved, ...future]);
    setDraft(previous);
    onUpdate({ id: placement.id, parts: previous });
  };
  const redo = () => {
    const next = future[0];
    if (!next) return;
    setFuture(future.slice(1));
    setPast([...past, saved]);
    setDraft(next);
    onUpdate({ id: placement.id, parts: next });
  };

  // ----- dragging and resizing -----

  const gesture = useRef<{ id: string; mode: "move" | "resize"; right: boolean; bottom: boolean; startX: number; startY: number; origin: Rect; moved: boolean; latest: Part[] | null; frame: number } | null>(null);
  const begin = (event: ReactPointerEvent<HTMLElement>, part: Part, mode: "move" | "resize", right = false, bottom = false) => {
    event.stopPropagation();
    onPick(part.id);
    box.current?.focus();
    if (locked || event.button !== 0) return;
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // The gesture still works while the pointer stays over the part.
    }
    gesture.current = { id: part.id, mode, right, bottom, startX: event.clientX, startY: event.clientY, origin: shown(part), moved: false, latest: null, frame: 0 };
  };
  const drag = (event: ReactPointerEvent<HTMLElement>) => {
    const state = gesture.current;
    if (!state) return;
    event.stopPropagation();
    const dx = (event.clientX - state.startX) / zoom;
    const dy = (event.clientY - state.startY) / zoom;
    if (!state.moved && Math.hypot(event.clientX - state.startX, event.clientY - state.startY) < 3) return;
    state.moved = true;
    const part = parts.find((item) => item.id === state.id);
    if (!part) return;
    const { origin } = state;
    let rect: Rect;
    let lines: Guides = { x: null, y: null };
    if (state.mode === "move") {
      // Line up with the widget's edges and middle, and with the other parts.
      const others = parts.filter((item) => item.id !== state.id && !item.hidden).map(shown);
      const xs = [0, size[0] / 2, size[0], ...others.flatMap((o) => [o.x, o.x + o.width / 2, o.x + o.width])];
      const ys = [0, size[1] / 2, size[1], ...others.flatMap((o) => [o.y, o.y + o.height / 2, o.y + o.height])];
      const reach = event.altKey ? 0 : SNAP_REACH / zoom;
      const x = snapAxis(origin.x + dx, origin.width, xs, reach);
      const y = snapAxis(origin.y + dy, origin.height, ys, reach);
      rect = { ...origin, x: x.start, y: y.start };
      lines = { x: x.line, y: y.line };
    } else {
      const width = Math.max(MIN_PART, origin.width + (state.right ? dx : -dx));
      const height = Math.max(MIN_PART, origin.height + (state.bottom ? dy : -dy));
      rect = { x: state.right ? origin.x : origin.x + origin.width - width, y: state.bottom ? origin.y : origin.y + origin.height - height, width, height };
    }
    const next = parts.map((item) => (item.id === state.id ? { ...item, ...stored(item, rect) } : item));
    state.latest = next;
    setGuides(lines);
    setDraft(next);
    if (!state.frame) {
      state.frame = requestAnimationFrame(() => {
        state.frame = 0;
        if (gesture.current === state && state.latest) onUpdate({ id: placement.id, parts: state.latest, dragging: true });
      });
    }
  };
  const drop = (event: ReactPointerEvent<HTMLElement>) => {
    const state = gesture.current;
    gesture.current = null;
    if (!state) return;
    event.stopPropagation();
    cancelAnimationFrame(state.frame);
    setGuides({ x: null, y: null });
    if (state.latest) commit(state.latest);
  };

  // ----- adding, removing, ordering -----

  const freshId = (base: string) => {
    let id = base;
    for (let n = 2; parts.some((part) => part.id === id); n++) id = `${base}-${n}`;
    return id;
  };
  const blank: Part = { id: "", name: "", kind: "text", source: "", text: "", x: 16, y: 16, width: 120, height: 18, stretch: false, fromBottom: false, font: null, size: 13, weight: "regular", color: "text", opacity: 100, align: "left", hidden: false, radius: 0 };
  const add = (part: Partial<Part> & { id: string; name: string }) => {
    setAdding(false);
    const made = { ...blank, ...part, id: freshId(part.id) };
    commit([...parts, made]);
    onPick(made.id);
  };
  const remove = () => {
    if (!picked) return;
    commit(parts.filter((part) => part.id !== picked.id));
    onPick(null);
  };
  const duplicate = () => {
    if (!picked) return;
    const copy = { ...picked, id: freshId(picked.id), name: `${picked.name} copy`, x: picked.x + 8, y: picked.y + 8 };
    commit([...parts, copy]);
    onPick(copy.id);
  };
  /** Later in the list is drawn on top. */
  const reorder = (step: 1 | -1) => {
    if (!picked) return;
    const index = parts.indexOf(picked);
    const target = index + step;
    if (target < 0 || target >= parts.length) return;
    const next = [...parts];
    [next[index], next[target]] = [next[target], next[index]];
    commit(next);
  };
  const nudge = (dx: number, dy: number) => {
    if (!picked) return;
    const rect = shown(picked);
    commit(parts.map((part) => (part.id === picked.id ? { ...part, ...stored(part, { ...rect, x: rect.x + dx, y: rect.y + dy }) } : part)));
  };

  const onKey = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (locked) return;
    const step = event.shiftKey ? 8 : 1;
    const moves: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
    if (moves[event.key] && picked) {
      event.preventDefault();
      nudge(...moves[event.key]);
    } else if ((event.key === "Delete" || event.key === "Backspace") && picked) {
      event.preventDefault();
      remove();
    } else if (event.ctrlKey && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (event.shiftKey) redo();
      else undo();
    } else if (event.ctrlKey && event.key.toLowerCase() === "y") {
      event.preventDefault();
      redo();
    } else if (event.ctrlKey && event.key.toLowerCase() === "d" && picked) {
      event.preventDefault();
      duplicate();
    } else if (event.key === "Escape") {
      onPick(null);
    }
  };

  const index = picked ? parts.indexOf(picked) : -1;
  const backdrop = { "--d-a": theme.roles.surface, "--d-b": theme.roles.neutral, "--d-c": theme.roles.primary } as CSSProperties;
  const { partDefaults } = useLumen();
  const sources = allSources(partDefaults);
  const numbers = sources.filter((item) => item.source.startsWith("stats.") || item.source === "media.progress");

  return (
    <div className="designer">
      <div className="designer-bar">
        <div className="menu-anchor">
          <Button size="bar" kind="primary" aria-expanded={adding} disabled={locked} onClick={() => setAdding((open) => !open)}>
            + Add a part
          </Button>
          {adding && (
            <div className="menu designer-menu" role="menu">
              <button type="button" role="menuitem" className="menu-item" onClick={() => add({ id: "text", name: "Text", text: "Text" })}>
                Text
              </button>
              {sources
                .filter((item) => item.source !== "media.art" && item.source !== "media.progress")
                .map((item) => (
                  <button key={item.source} type="button" role="menuitem" className="menu-item" onClick={() => add({ id: "value", name: item.label, text: `{${item.source}}` })}>
                    Live value: {item.label}
                  </button>
                ))}
              {numbers.map((item) => (
                <button key={item.source} type="button" role="menuitem" className="menu-item" onClick={() => add({ id: "bar", name: `${item.label} bar`, kind: "bar", source: item.source, height: 5, color: "highlight" })}>
                  Level bar: {item.label}
                </button>
              ))}
              <button type="button" role="menuitem" className="menu-item" onClick={() => add({ id: "shape", name: "Shape", kind: "shape", width: 48, height: 48, radius: 10, color: "neutral" })}>
                Shape
              </button>
              <button type="button" role="menuitem" className="menu-item" onClick={() => add({ id: "divider", name: "Divider", kind: "shape", y: Math.round(size[1] / 2), width: Math.max(8, defaultSize[0] - 32), height: 1, stretch: true, color: "outline" })}>
                Divider
              </button>
            </div>
          )}
        </div>
        <Button size="bar" disabled={!picked || locked} onClick={duplicate}>
          Duplicate
        </Button>
        <Button size="bar" disabled={!picked || locked || index >= parts.length - 1} onClick={() => reorder(1)}>
          Forward
        </Button>
        <Button size="bar" disabled={!picked || locked || index <= 0} onClick={() => reorder(-1)}>
          Back
        </Button>
        <Button size="bar" kind="danger" disabled={!picked || locked} onClick={remove}>
          Remove
        </Button>
        <span className="designer-gap" />
        <Button size="bar" disabled={past.length === 0} onClick={undo}>
          Undo
        </Button>
        <Button size="bar" disabled={future.length === 0} onClick={redo}>
          Redo
        </Button>
        {file ? (
          <Button size="bar" aria-pressed={code} onClick={() => setCode((open) => !open)}>
            {code ? "Hide text" : "Edit as text"}
          </Button>
        ) : (
          <Button size="bar" disabled={!placement.parts || locked} onClick={() => commit([])}>
            Reset
          </Button>
        )}
        <Button size="bar" kind="primary" onClick={onDone}>
          Done
        </Button>
      </div>

      <div
        ref={box}
        className="designer-canvas"
        style={backdrop}
        tabIndex={0}
        role="application"
        aria-label={`Designer for this widget: ${parts.length} parts. Arrow keys move the selected part.`}
        onKeyDown={onKey}
        onPointerDown={() => {
          onPick(null);
          setAdding(false);
        }}
      >
        <div className="designer-stage" style={{ width: size[0] * zoom, height: size[1] * zoom }}>
          <div className="designer-widget" style={{ transform: `scale(${zoom})` }}>
            <WidgetView parts={parts} look={look} size={size} defaultSize={defaults.size} values={values} />
          </div>
          {guides.x != null && <span className="designer-guide designer-guide-x" style={{ left: guides.x * zoom }} />}
          {guides.y != null && <span className="designer-guide designer-guide-y" style={{ top: guides.y * zoom }} />}
          {parts.map((part) => {
            const rect = shown(part);
            const isPicked = part.id === pickedId;
            if (part.hidden && !isPicked) return null;
            return (
              <div
                key={part.id}
                className={`designer-part${isPicked ? " designer-part-picked" : ""}${part.hidden ? " designer-part-hidden" : ""}`}
                style={{ left: rect.x * zoom, top: rect.y * zoom, width: rect.width * zoom, height: Math.max(rect.height * zoom, 6) }}
                title={`${part.name} (${PART_KIND_NAMES[part.kind]})`}
                onPointerDown={(event) => begin(event, part, "move")}
                onPointerMove={drag}
                onPointerUp={drop}
                onPointerCancel={drop}
              >
                {isPicked &&
                  !locked &&
                  (["tl", "tr", "bl", "br"] as const).map((corner) => (
                    <span
                      key={corner}
                      className={`pw-handle pw-handle-${corner}`}
                      onPointerDown={(event) => begin(event, part, "resize", corner.endsWith("r"), corner.startsWith("b"))}
                      onPointerMove={drag}
                      onPointerUp={drop}
                      onPointerCancel={drop}
                    />
                  ))}
              </div>
            );
          })}
        </div>
      </div>
      <p className="muted small-13">
        {picked
          ? `${picked.name}: drag to move, drag a corner to resize. Arrow keys move it by one pixel (Shift for eight); hold Alt while dragging to stop it snapping. Its font, colour and text are in the Parts panel on the right.`
          : "Click a part to select it. Your real widget on the desktop changes as you work, and everything is saved as you go."}
      </p>
      {file && code && <CodeView file={file} />}
    </div>
  );
}
