// The gallery: looks, styles and widgets other people have shared. Each is
// downloaded and added through the same door as a file you pick yourself.
// A widget with a script shows what the script asks for before it is added,
// and the script still does not run until you allow it on the Widgets screen.

import { useCallback, useEffect, useState } from "react";
import { PageHeader } from "../Shell";
import { useLumen } from "../store";
import type { GalleryInstall, GalleryItem, GalleryKind, GalleryState, ScriptPermissions } from "../types";
import { Button } from "../ui";

const GROUPS: { kind: GalleryKind; title: string; added: string }[] = [
  { kind: "preset", title: "Looks", added: "Added to your presets." },
  { kind: "widget", title: "Widgets", added: "Added to your widget files, on the Widgets screen." },
  { kind: "style", title: "Component styles", added: "Added to your component styles, under Theme & colours." },
];

function asks(permissions: ScriptPermissions): string[] {
  const list: string[] = [];
  if (permissions.files) list.push("Read and change files on this PC");
  if (permissions.network) list.push("Go online");
  if (permissions.run) list.push("Run and open programs");
  return list;
}

export function Gallery() {
  const { request, go } = useLumen();
  const [gallery, setGallery] = useState<GalleryState | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  /** What happened to each item this visit: added, or waiting for a yes about its script. */
  const [outcome, setOutcome] = useState<Record<string, GalleryInstall>>({});
  const [editing, setEditing] = useState(false);
  const [source, setSource] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    const state = await request<GalleryState>("getGallery");
    setGallery(state);
    setLoading(false);
  }, [request]);
  useEffect(() => void load(), [load]);

  const useSource = async (chosen: string = source.trim()) => {
    setLoading(true);
    const state = await request<GalleryState>("setGallerySource", { source: chosen });
    setLoading(false);
    if (!state) return;
    setGallery(state);
    setOutcome({});
    setEditing(false);
  };

  const install = async (item: GalleryItem, ok: boolean) => {
    setBusy(item.id);
    const result = await request<GalleryInstall>("installGalleryItem", { id: item.id, ok });
    setBusy(null);
    if (result) setOutcome((all) => ({ ...all, [item.id]: result }));
  };
  const forget = (id: string) =>
    setOutcome((all) => {
      const { [id]: _gone, ...rest } = all;
      return rest;
    });

  const sourceForm = (
    <form
      className="gallery-source"
      onSubmit={(event) => {
        event.preventDefault();
        void useSource();
      }}
    >
      <input className="parts-text" type="text" aria-label="Gallery address or folder" placeholder="https://… or C:\…\gallery" value={source} onChange={(event) => setSource(event.target.value)} />
      <Button size="bar" kind="primary" type="submit" disabled={loading}>
        Use this gallery
      </Button>
      {gallery?.source && (
        <Button size="bar" onClick={() => setEditing(false)}>
          Cancel
        </Button>
      )}
    </form>
  );

  return (
    <>
      <PageHeader title="Gallery" tip="looks, styles and widgets other people have shared">
        {gallery?.source && (
          <div className="header-actions">
            <Button size="header" className="btn-plain" disabled={loading} onClick={() => void load()}>
              {loading ? "Reading…" : "Refresh"}
            </Button>
          </div>
        )}
      </PageHeader>

      {!gallery && loading && <p className="muted">Reading the gallery…</p>}

      {gallery && !gallery.source && (
        <section className="panel" aria-label="No gallery yet">
          <h2 className="panel-title">No gallery was found</h2>
          <p className="muted">The gallery that comes with Lumen is missing from this copy. If someone has shared a gallery with you, enter its address, or the folder it is in, to browse it.</p>
          {sourceForm}
        </section>
      )}

      {gallery?.error && (
        <div className="strip strip-danger" role="alert">
          <span>{gallery.error}</span>
        </div>
      )}

      {gallery?.source && !gallery.error && gallery.items.length === 0 && !loading && <p className="muted">This gallery is empty.</p>}

      {gallery?.source &&
        GROUPS.map((group) => {
          const items = gallery.items.filter((item) => item.kind === group.kind);
          if (items.length === 0) return null;
          return (
            <section key={group.kind} className="gallery-group" aria-label={group.title}>
              <h2 className="panel-title">{group.title}</h2>
              <div className="gallery-grid">
                {items.map((item) => {
                  const result = outcome[item.id];
                  const wanted = result?.needsOk ? asks(result.permissions) : [];
                  return (
                    <article key={item.id} className="gallery-item">
                      <div className="field-row baseline wrap">
                        <strong>{item.name}</strong>
                        {item.script && <span className="gallery-tag">Has a script</span>}
                      </div>
                      {item.author && <span className="muted small-13">by {item.author}</span>}
                      {item.description && <p className="small-14">{item.description}</p>}
                      {result?.installed ? (
                        <p className="small-13 gallery-added" role="status">
                          {result.already ? "You already have this one; nothing new was added." : group.added}
                          {result.script ? " Its script will not run until you allow it there." : ""}{" "}
                          {group.kind !== "style" && (
                            <button type="button" className="link small-13" onClick={() => go(group.kind === "widget" ? "widgets" : "presets")}>
                              Go there
                            </button>
                          )}
                        </p>
                      ) : result?.needsOk ? (
                        <div className="gallery-ask" role="group" aria-label={`About the script in ${item.name}`}>
                          <p className="small-13">
                            This widget comes with a script (<strong>{result.script}</strong>).{" "}
                            {wanted.length > 0 ? "If you later allow the script, it will be able to:" : "The script asks for nothing beyond Lumen's own live values: it cannot touch files, go online or run programs."}
                          </p>
                          {wanted.length > 0 && (
                            <ul className="script-asks small-13">
                              {wanted.map((line) => (
                                <li key={line}>{line}</li>
                              ))}
                            </ul>
                          )}
                          <p className="muted small">Adding the widget does not run the script. You can read it and allow it, or not, on the Widgets screen.</p>
                          <span className="header-actions">
                            <Button size="bar" kind="primary" disabled={busy !== null} onClick={() => void install(item, true)}>
                              {busy === item.id ? "Adding…" : "Add the widget"}
                            </Button>
                            <Button size="bar" disabled={busy !== null} onClick={() => forget(item.id)}>
                              Cancel
                            </Button>
                          </span>
                        </div>
                      ) : (
                        <Button size="bar" className="gallery-add" disabled={busy !== null} onClick={() => void install(item, false)}>
                          {busy === item.id ? "Downloading…" : "Add to Lumen"}
                        </Button>
                      )}
                    </article>
                  );
                })}
              </div>
            </section>
          );
        })}

      {gallery?.source && !editing && (
        <section className="strip gallery-where" aria-label="Which gallery">
          {gallery.local ? (
            <>
              <span>These came with Lumen and are read from this PC. Nothing has gone online.</span>
              <Button size="bar" disabled={loading} onClick={() => void useSource(gallery.online)}>
                Browse the online gallery
              </Button>
            </>
          ) : (
            <>
              <span>{gallery.source === gallery.online ? "This is the online gallery: things other people have shared, each looked at before it was listed." : "This is a gallery someone shared with you."}</span>
              <Button size="bar" disabled={loading} onClick={() => void useSource("")}>
                Back to the ones that came with Lumen
              </Button>
            </>
          )}
        </section>
      )}

      {gallery?.source && (
        <footer className="gallery-footer muted small-13">
          {editing ? (
            sourceForm
          ) : (
            <>
              <span>
                {gallery.name ? `${gallery.name} · ` : ""}
                {gallery.source}
              </span>{" "}
              <button
                type="button"
                className="link small-13"
                onClick={() => {
                  setSource(gallery.source ?? "");
                  setEditing(true);
                }}
              >
                Browse a gallery someone shared…
              </button>
            </>
          )}
        </footer>
      )}
    </>
  );
}
