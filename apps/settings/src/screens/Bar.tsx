// Bar, dock & menus: turn the status bar on, place it, and choose what sits in
// its left, centre and right slots.

import { open } from "@tauri-apps/plugin-dialog";
import { useState, type CSSProperties, type ReactNode } from "react";
import { PageHeader } from "../Shell";
import { useLumen } from "../store";
import { BAR_HEIGHT_RANGE, MAX_LAUNCHER_APPS, MAX_SLOT_MODULES, type BarConfig, type BarModule, type BarSlot, type LauncherApp, type Theme } from "../types";
import { Button, Check, Segmented, Slider, Tip } from "../ui";

const SLOTS: { slot: BarSlot; label: string }[] = [
  { slot: "left", label: "Left" },
  { slot: "centre", label: "Centre" },
  { slot: "right", label: "Right" },
];

const MODULES: { type: BarModule["type"]; label: string; about: string }[] = [
  { type: "lumen", label: "Lumen", about: "Click opens this app" },
  { type: "clock", label: "Clock", about: "The time" },
  { type: "date", label: "Date", about: "Day and date" },
  { type: "cpu", label: "CPU", about: "Processor use" },
  { type: "memory", label: "Memory", about: "RAM in use" },
  { type: "gpu", label: "GPU", about: "Graphics card use" },
  { type: "media", label: "Now playing", about: "Click plays or pauses" },
  { type: "launcher", label: "App launcher", about: "Icons that open your apps" },
];
const LABEL = Object.fromEntries(MODULES.map((m) => [m.type, m.label])) as Record<BarModule["type"], string>;

const PROGRAMS = [{ name: "Programs and shortcuts", extensions: ["exe", "lnk", "url"] }];

interface Pick {
  slot: BarSlot;
  index: number;
}

export function BarScreen() {
  const { bar, setBar, monitors, shown, status } = useLumen();
  const [picked, setPicked] = useState<Pick | null>(null);
  const [adding, setAdding] = useState<BarSlot | null>(null);

  if (!bar) {
    return (
      <>
        <PageHeader title="Bar, dock & menus" />
        <section className="card coming-soon">
          <p>The bar is not available right now.</p>
          <p className="muted">It runs alongside the desktop widgets, which could not start. Restart Lumen from the tray and try again.</p>
        </section>
      </>
    );
  }

  const change = (patch: Partial<BarConfig>) => void setBar({ ...bar, ...patch });
  const pickedModule = picked ? (bar[picked.slot][picked.index] ?? null) : null;
  const order = SLOTS.map((s) => s.slot);

  const add = (slot: BarSlot, type: BarModule["type"]) => {
    setAdding(null);
    const module: BarModule = type === "launcher" ? { type, apps: [] } : { type };
    change({ [slot]: [...bar[slot], module] });
    setPicked({ slot, index: bar[slot].length });
  };

  const remove = ({ slot, index }: Pick) => {
    change({ [slot]: bar[slot].filter((_, i) => i !== index) });
    setPicked(null);
  };

  /** Where a module lands when moved one step; null if it cannot move that way. */
  const destination = ({ slot, index }: Pick, step: -1 | 1): Pick | null => {
    const within = index + step;
    if (within >= 0 && within < bar[slot].length) return { slot, index: within };
    // Off the end of its slot: into the neighbouring slot, if there is one with room.
    const next = order[order.indexOf(slot) + step];
    if (!next || bar[next].length >= MAX_SLOT_MODULES) return null;
    return { slot: next, index: step === 1 ? 0 : bar[next].length };
  };

  const move = (from: Pick, step: -1 | 1) => {
    const to = destination(from, step);
    if (!to) return;
    const module = bar[from.slot][from.index];
    if (to.slot === from.slot) {
      const list = [...bar[from.slot]];
      list.splice(from.index, 1);
      list.splice(to.index, 0, module);
      change({ [from.slot]: list });
    } else {
      const target = [...bar[to.slot]];
      target.splice(to.index, 0, module);
      change({ [from.slot]: bar[from.slot].filter((_, i) => i !== from.index), [to.slot]: target });
    }
    setPicked(to);
  };

  const setApps = (at: Pick, apps: LauncherApp[]) => {
    change({ [at.slot]: bar[at.slot].map((m, i) => (i === at.index ? { type: "launcher" as const, apps } : m)) });
  };

  const addApps = async (at: Pick, current: LauncherApp[]) => {
    const chosen = await open({ multiple: true, title: "Add apps to the bar", filters: PROGRAMS });
    if (!chosen) return;
    const paths = (Array.isArray(chosen) ? chosen : [chosen]).filter((path) => !current.some((app) => app.path === path));
    setApps(at, [...current, ...paths.map((path) => ({ name: appName(path), path }))].slice(0, MAX_LAUNCHER_APPS));
  };

  const sorted = [...monitors].sort((a, b) => a.number - b.number);
  const monitorKnown = bar.monitor === "main" || bar.monitor === "all" || sorted.some((m) => `#${m.number}` === bar.monitor);

  return (
    <>
      <PageHeader title="Bar, dock & menus" tip={bar.enabled ? "it follows your theme" : "off for now"}>
        <div className="header-actions header-actions-center">
          <Segmented
            label="Status bar"
            look="track"
            value={bar.enabled ? "on" : "off"}
            onChange={(value) => change({ enabled: value === "on" })}
            options={[
              { value: "off", label: "Bar off" },
              { value: "on", label: "Bar on" },
            ]}
          />
        </div>
      </PageHeader>

      {bar.enabled && status?.gameMode.active && (
        <div className="strip" role="status">
          <span>A fullscreen app is in front, so the bar is put away until you leave it.</span>
        </div>
      )}

      <div className="split bar-split">
        <div className="bar-main">
          <div className="label">Preview</div>
          {shown ? <BarPreview bar={bar} theme={shown} picked={picked} /> : <div className="empty">Pick a theme to see the bar in its colours.</div>}

          <div className="label bar-slots-label">Slots</div>
          <div className="bar-slots">
            {SLOTS.map(({ slot, label }) => (
              <section key={slot} className="panel bar-slot" aria-label={`${label} slot`}>
                <div className="field-row">
                  <h2 className="panel-title">{label}</h2>
                  <div className="menu-anchor">
                    <Button
                      size="bar"
                      aria-expanded={adding === slot}
                      disabled={bar[slot].length >= MAX_SLOT_MODULES}
                      onClick={() => setAdding((current) => (current === slot ? null : slot))}
                    >
                      + Add
                    </Button>
                    {adding === slot && (
                      <div className="menu" role="menu">
                        {MODULES.map((item) => (
                          <button key={item.type} type="button" role="menuitem" className="menu-item bar-menu-item" onClick={() => add(slot, item.type)}>
                            <span>{item.label}</span>
                            <span className="muted small">{item.about}</span>
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
                {bar[slot].length === 0 && <p className="muted small-14 bar-slot-empty">Nothing here yet.</p>}
                <ul className="bar-modules">
                  {bar[slot].map((module, index) => {
                    const at = { slot, index };
                    const isPicked = picked?.slot === slot && picked.index === index;
                    return (
                      <li key={`${module.type}-${index}`} className={`bar-module${isPicked ? " bar-module-picked" : ""}`}>
                        <button type="button" className="bar-module-name" aria-pressed={isPicked} onClick={() => setPicked(isPicked ? null : at)}>
                          <span>{LABEL[module.type]}</span>
                          {module.type === "launcher" && <span className="muted small">{appCount(module.apps.length)}</span>}
                        </button>
                        <span className="bar-module-actions">
                          <IconButton label={`Move ${LABEL[module.type]} left`} disabled={!destination(at, -1)} onClick={() => move(at, -1)}>
                            <path d="M14 6 L8 12 L14 18" />
                          </IconButton>
                          <IconButton label={`Move ${LABEL[module.type]} right`} disabled={!destination(at, 1)} onClick={() => move(at, 1)}>
                            <path d="M10 6 L16 12 L10 18" />
                          </IconButton>
                          <IconButton label={`Remove ${LABEL[module.type]}`} onClick={() => remove(at)}>
                            <path d="M7 7 L17 17 M17 7 L7 17" />
                          </IconButton>
                        </span>
                      </li>
                    );
                  })}
                </ul>
              </section>
            ))}
          </div>

          {picked && pickedModule?.type === "launcher" && (
            <section className="panel" aria-label="Launcher apps">
              <div className="field-row">
                <h2 className="panel-title">Launcher apps</h2>
                <Button size="bar" disabled={pickedModule.apps.length >= MAX_LAUNCHER_APPS} onClick={() => void addApps(picked, pickedModule.apps)}>
                  + Add apps
                </Button>
              </div>
              {pickedModule.apps.length === 0 ? (
                <p className="muted small-14">Add programs or shortcuts, and their icons appear in the bar. A launcher with no apps is not shown.</p>
              ) : (
                <ul className="bar-apps">
                  {pickedModule.apps.map((app, index) => (
                    <li key={app.path} className="bar-app">
                      <span className="stack-2 bar-app-text">
                        <span>{app.name}</span>
                        <span className="muted small bar-app-path">{app.path}</span>
                      </span>
                      <IconButton label={`Remove ${app.name}`} onClick={() => setApps(picked, pickedModule.apps.filter((_, i) => i !== index))}>
                        <path d="M7 7 L17 17 M17 7 L7 17" />
                      </IconButton>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          )}
        </div>

        <aside className="panel bar-side" aria-label="Bar settings">
          <h2 className="panel-title">Bar</h2>
          <div className="field">
            <span className="label">Position</span>
            <Segmented
              label="Position"
              look="grid-2"
              value={bar.position}
              onChange={(position) => change({ position })}
              options={[
                { value: "top", label: "Top" },
                { value: "bottom", label: "Bottom" },
              ]}
            />
          </div>
          <Slider label="Height" value={bar.height} min={BAR_HEIGHT_RANGE[0]} max={BAR_HEIGHT_RANGE[1]} step={2} format={(v) => `${v} px`} onCommit={(height) => change({ height })} />
          <div className="field">
            <label className="label" htmlFor="bar-monitor">
              Monitor
            </label>
            <select id="bar-monitor" value={bar.monitor} onChange={(event) => change({ monitor: event.target.value })}>
              <option value="main">Main monitor</option>
              {sorted.length > 1 && <option value="all">Every monitor</option>}
              {sorted.map((m) => (
                <option key={m.id} value={`#${m.number}`}>
                  Monitor {m.number}
                  {m.primary ? " (main)" : ""} · {m.width} × {m.height}
                </option>
              ))}
              {!monitorKnown && <option value={bar.monitor}>Monitor {bar.monitor.replace("#", "")} (not connected)</option>}
            </select>
          </div>
          <Check checked={bar.reserveSpace} onChange={(reserveSpace) => change({ reserveSpace })}>
            Keep windows clear of the bar
          </Check>
          <p className="muted small-13">
            {bar.reserveSpace ? "Maximised windows stop at the bar, as they do at the taskbar." : "The bar floats over whatever is underneath it."} It is put away while a fullscreen game runs.
          </p>

          <div className="bar-later">
            <span className="label">Still to come</span>
            <span className="muted small-13">Auto-hide, side bars, themed flyouts, the launcher hotkey and the tray menu preview.</span>
            <Tip accent size={20}>
              arriving after the bar
            </Tip>
          </div>
        </aside>
      </div>
    </>
  );
}

function appCount(count: number): string {
  return count === 1 ? "1 app" : `${count} apps`;
}

/** "C:\Apps\Paint.exe" → "Paint". */
function appName(path: string): string {
  const file = path.split(/[\\/]/).pop() ?? path;
  return file.replace(/\.(exe|lnk|url)$/i, "") || file;
}

function IconButton({ label, disabled, onClick, children }: { label: string; disabled?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" className="bar-icon-button" aria-label={label} title={label} disabled={disabled} onClick={onClick}>
      <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true" fill="none" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        {children}
      </svg>
    </button>
  );
}

/** The bar as it looks on the desktop, in the theme's own colours, with sample values. */
function BarPreview({ bar, theme, picked }: { bar: BarConfig; theme: Theme; picked: Pick | null }) {
  const r = theme.roles;
  const vars = {
    "--t-primary": r.primary,
    "--t-neutral": r.neutral,
    "--t-surface": r.surface,
    "--t-on-surface": r.onSurface,
    "--t-outline": r.outline,
    "--t-font": `'${theme.style.font}', var(--font-body)`,
    "--bar-height": `${bar.height}px`,
  } as CSSProperties;
  const now = new Date();
  const sample = (module: BarModule) => {
    switch (module.type) {
      case "lumen":
        return <span className="bar-pv-strong">Lumen</span>;
      case "clock":
        return <span className="bar-pv-medium">{now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</span>;
      case "date":
        return <span>{now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}</span>;
      case "cpu":
        return <Stat label="CPU" value="12%" />;
      case "memory":
        return <Stat label="RAM" value="41%" />;
      case "gpu":
        return <Stat label="GPU" value="8%" />;
      case "media":
        return (
          <span className="bar-pv-media">
            <span className="bar-pv-dot" />
            Track title · Artist
          </span>
        );
      case "launcher":
        return (
          <span className="bar-pv-icons">
            {module.apps.map((app) => (
              <span key={app.path} className="bar-pv-icon" title={app.name}>
                {app.name.charAt(0).toUpperCase()}
              </span>
            ))}
          </span>
        );
    }
  };
  const visible = (module: BarModule) => module.type !== "launcher" || module.apps.length > 0;

  return (
    <div className={`bar-pv bar-pv-${bar.position}${bar.enabled ? "" : " bar-pv-off"}`} style={vars} role="img" aria-label={`Preview of the bar at the ${bar.position} of the screen`}>
      <div className="bar-pv-strip">
        {SLOTS.map(({ slot }) => (
          <div key={slot} className={`bar-pv-slot bar-pv-${slot}`}>
            {bar[slot].map(
              (module, index) =>
                visible(module) && (
                  <span key={`${module.type}-${index}`} className={picked?.slot === slot && picked.index === index ? "bar-pv-picked" : undefined}>
                    {sample(module)}
                  </span>
                ),
            )}
          </div>
        ))}
      </div>
      <div className="bar-pv-desktop">{bar.enabled ? "your desktop" : "the bar is off"}</div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <span>
      <span className="bar-pv-muted">{label}</span> <span className="bar-pv-medium">{value}</span>
    </span>
  );
}
