// Home: the active look, quick switching, and the status strip.

import { PageHeader, SearchBox } from "../Shell";
import { useLumen } from "../store";
import type { Theme } from "../types";
import { Arrow, Button, Dots, Tip, WallpaperArt } from "../ui";

/** The five dots on the hero card. */
function heroColours(theme: Theme): string[] {
  const r = theme.roles;
  return [r.primary, r.secondary, r.tertiary, r.accent, r.surface];
}

export function Home() {
  const { shown, wallpapers, wallpaperOf, previewTheme, go, status, connected } = useLumen();
  const active = wallpaperOf(shown);

  // Until presets exist (M7), Quick switch offers the newest wallpapers as one-click looks.
  const quick = [...wallpapers]
    .filter((w) => w.id !== active?.id)
    .sort((a, b) => b.addedAt - a.addedAt)
    .slice(0, 3);

  const shuffle = () => {
    const others = wallpapers.filter((w) => w.id !== active?.id);
    const pick = others[Math.floor(Math.random() * others.length)];
    if (pick) void previewTheme({ source: { type: "wallpaper", path: pick.file } });
  };

  return (
    <>
      <PageHeader title="Home">
        <SearchBox />
      </PageHeader>

      <section className="hero card" aria-label="Active preset">
        <WallpaperArt
          className="hero-art"
          wallpaper={active}
          colours={shown ? heroColours(shown) : undefined}
          label={shown ? `Preview of the active wallpaper, ${shown.name}` : "No wallpaper chosen yet"}
        />
        <div className="hero-info">
          <div className="label">Active preset</div>
          {shown ? (
            <>
              <h2 className="hero-title">{shown.name}</h2>
              <div className="hero-dots">
                <Dots colours={heroColours(shown)} size={30} />
                <Arrow />
                <Tip accent size={22}>
                  {shown.source.type === "wallpaper" ? "from your wallpaper" : shown.source.type === "color" ? "from your colour" : "picked by hand"}
                </Tip>
              </div>
              <div className="button-row">
                <Button kind="primary" onClick={() => go("theme")}>
                  Edit preset
                </Button>
                <Button onClick={shuffle} disabled={wallpapers.filter((w) => w.id !== active?.id).length === 0}>
                  Shuffle
                </Button>
              </div>
            </>
          ) : (
            <>
              <h2 className="hero-title">No look yet</h2>
              <p className="muted hero-empty">Pick a wallpaper and Lumen builds a matching look for your whole desktop.</p>
              <div className="button-row">
                <Button kind="primary" onClick={() => go("wallpapers")}>
                  Pick a wallpaper
                </Button>
                <Button onClick={() => go("theme")}>Start from a colour</Button>
              </div>
            </>
          )}
        </div>
      </section>

      <section className="quick" aria-label="Quick switch">
        <div className="section-heading">
          <h3>Quick switch</h3>
          <Tip size={20}>one click, whole desktop changes</Tip>
        </div>
        <div className="quick-row">
          {quick.map((wallpaper) => (
            <button key={wallpaper.id} type="button" className="quick-card" onClick={() => void previewTheme({ source: { type: "wallpaper", path: wallpaper.file } })}>
              <WallpaperArt className="quick-art" wallpaper={wallpaper} />
              <span className="quick-name">{wallpaper.name}</span>
            </button>
          ))}
          {quick.length === 0 && (
            <button type="button" className="quick-card quick-empty" onClick={() => go("wallpapers")}>
              <span className="quick-name">Add wallpapers to switch between them here</span>
            </button>
          )}
        </div>
      </section>

      <div className="status-strip" role="status">
        <span className={connected ? "status-dot" : "status-dot status-dot-danger"} />
        <span>{statusLine(connected, status)}</span>
      </div>
    </>
  );
}

function statusLine(connected: boolean, status: ReturnType<typeof useLumen>["status"]): string {
  if (!connected || !status) return "Lumen's core is not running";
  const memory = `Memory ${status.memoryMb.toFixed(0)} MB`;
  const cpu = `CPU ${status.cpuPercent.toFixed(1)}%`;
  const game = status.gameMode.active ? `Game mode: paused for “${status.gameMode.app ?? "a fullscreen app"}”` : "Game mode: auto-pause on";
  return `${memory} · ${cpu} · ${game}`;
}
