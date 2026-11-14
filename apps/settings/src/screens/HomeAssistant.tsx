// Home Assistant: the connection (address and access token), and the list of
// things it knows about, to pick from. The token is sent to Lumen's
// background program once, saved on this PC, and never shown again.

import { useEffect, useMemo, useState } from "react";
import { useLumen } from "../store";
import type { HaEntity, HaState } from "../types";
import { Button } from "../ui";

function statusLine(state: HaState): { text: string; bad: boolean } {
  switch (state.status) {
    case "connected":
      return { text: `Connected${state.name ? ` to ${state.name}` : ""}.`, bad: false };
    case "checking":
      return { text: "Trying the connection…", bad: false };
    case "refused":
    case "unreachable":
      return { text: state.error ?? "Could not connect.", bad: true };
    default:
      return { text: "Not connected yet.", bad: false };
  }
}

/** The connection box, for the Settings screen. */
export function HaConnection() {
  const { homeAssistant, request } = useLumen();
  const [address, setAddress] = useState("");
  const [token, setToken] = useState("");
  // Start from what is saved, once it is known.
  useEffect(() => {
    if (homeAssistant) setAddress((typed) => typed || homeAssistant.address);
  }, [homeAssistant]);
  if (!homeAssistant) return null;
  const status = statusLine(homeAssistant);
  const connect = async () => {
    await request("setHomeAssistant", { address, token: token || null });
    setToken("");
  };

  return (
    <section className="panel" aria-label="Home Assistant">
      <h2 className="panel-title">Home Assistant</h2>
      <p className="muted small-13">Show anything Home Assistant knows about on a widget, and switch lights, plugs, scenes and the rest with a click.</p>
      <form
        className="ha-form"
        onSubmit={(event) => {
          event.preventDefault();
          void connect();
        }}
      >
        <label className="field">
          <span className="small-14">Address</span>
          <input className="parts-text" type="text" placeholder="http://homeassistant.local:8123" value={address} onChange={(event) => setAddress(event.target.value)} />
          <span className="muted small">What you type into a browser to open Home Assistant.</span>
        </label>
        <label className="field">
          <span className="small-14">Access token</span>
          <input className="parts-text" type="password" autoComplete="off" placeholder={homeAssistant.hasToken ? "Saved. Paste a new one to replace it" : "Paste a long-lived access token"} value={token} onChange={(event) => setToken(event.target.value)} />
          <span className="muted small">
            In Home Assistant: your profile, then Security, then “Long-lived access tokens”, then Create token. Copy the whole token straight away; Home Assistant shows it only once.{" "}
            <button type="button" className="link small" disabled={!address.trim()} onClick={() => void request("openHaTokens", { address })}>
              Open that page
            </button>
          </span>
        </label>
        <div className="field-row wrap">
          <span role="status" className={`small-14${status.bad ? " script-problem" : ""}`}>
            {status.text}
          </span>
          <span className="header-actions">
            {homeAssistant.hasToken && (
              <Button size="bar" onClick={() => void request("setHomeAssistant", { address: "", token: null }).then(() => setAddress(""))}>
                Disconnect
              </Button>
            )}
            <Button size="bar" kind="primary" type="submit" disabled={!address.trim() || (!token.trim() && !homeAssistant.hasToken) || homeAssistant.status === "checking"}>
              {homeAssistant.hasToken && !token ? "Try again" : "Connect"}
            </Button>
          </span>
        </div>
      </form>
      <p className="muted small">The token is kept on this PC only. It is never part of a widget, a preset or anything you share. Lumen asks Home Assistant only about things a widget on screen shows.</p>
    </section>
  );
}

/** The most things one widget takes at once (the core's limit). */
const MOST = 12;

/**
 * The list of everything in Home Assistant. Tick one or several, then make a
 * widget from them, or add them to the widget that is selected.
 */
export function HaPicker({ onMake, addTo, onClose }: { onMake: (entities: HaEntity[], title: string) => void | Promise<void>; addTo: { name: string; add: (entities: HaEntity[]) => void | Promise<void> } | null; onClose: () => void }) {
  const { homeAssistant, request, go } = useLumen();
  const [entities, setEntities] = useState<HaEntity[] | null>(null);
  const [search, setSearch] = useState("");
  const [only, setOnly] = useState<"all" | "switch" | "read">("all");
  const [picked, setPicked] = useState<string[]>([]);
  const [title, setTitle] = useState("Home");
  const [busy, setBusy] = useState(false);
  const connected = homeAssistant?.status === "connected";
  useEffect(() => {
    if (!connected) return;
    let current = true;
    void request<HaEntity[]>("listHaEntities").then((list) => {
      if (current) setEntities(list ?? []);
    });
    return () => {
      current = false;
    };
  }, [connected, request]);

  const shown = useMemo(() => {
    const words = search.toLowerCase().split(/\s+/).filter(Boolean);
    return (entities ?? []).filter((entity) => (only === "all" || (only === "switch") === entity.clickable) && words.every((word) => entity.name.toLowerCase().includes(word) || entity.id.includes(word)));
  }, [entities, search, only]);
  // In the order they were ticked, which is the order of the rows.
  const chosen = picked.map((id) => (entities ?? []).find((entity) => entity.id === id)).filter((entity): entity is HaEntity => !!entity);
  const toggle = (id: string) => setPicked((now) => (now.includes(id) ? now.filter((other) => other !== id) : now.length >= MOST ? now : [...now, id]));
  const run = async (work: () => void | Promise<void>) => {
    setBusy(true);
    try {
      await work();
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel ha-picker" aria-label="Add from Home Assistant">
      <div className="field-row wrap">
        <h2 className="panel-title">Add from Home Assistant</h2>
        <Button size="bar" onClick={onClose}>
          Close
        </Button>
      </div>
      {!connected ? (
        <p className="small-14">
          {homeAssistant?.status === "checking" ? "Trying the connection…" : (homeAssistant?.error ?? "Lumen is not connected to Home Assistant yet.")}{" "}
          <button type="button" className="link small-14" onClick={() => go("settings")}>
            Set it up under Settings
          </button>
        </p>
      ) : (
        <>
          <div className="ha-filter">
            <input className="parts-text" type="search" aria-label="Search Home Assistant" placeholder="Search by name: lamp, temperature, garage…" value={search} autoFocus onChange={(event) => setSearch(event.target.value)} />
            <select aria-label="What to list" value={only} onChange={(event) => setOnly(event.target.value as typeof only)}>
              <option value="all">Everything</option>
              <option value="switch">Things to switch or run</option>
              <option value="read">Things to read</option>
            </select>
          </div>
          {entities === null ? (
            <p className="muted small-14">Asking Home Assistant…</p>
          ) : shown.length === 0 ? (
            <p className="muted small-14">{entities.length === 0 ? "Home Assistant sent back nothing." : "Nothing matches."}</p>
          ) : (
            <ul className="ha-list">
              {shown.slice(0, 300).map((entity) => {
                const ticked = picked.includes(entity.id);
                return (
                  <li key={entity.id}>
                    <label className={`ha-row${ticked ? " ha-row-picked" : ""}`}>
                      <input type="checkbox" checked={ticked} disabled={!ticked && picked.length >= MOST} onChange={() => toggle(entity.id)} />
                      <span className="ha-name">{entity.name}</span>
                      <span className="muted small ha-id">{entity.id}</span>
                      <span className="small-13 ha-state">{entity.state}</span>
                      <span className="muted small ha-kind">{entity.clickable ? "Switch" : "Reading"}</span>
                    </label>
                  </li>
                );
              })}
            </ul>
          )}
          {entities && shown.length > 300 && <p className="muted small">Showing the first 300 of {shown.length}; search to narrow it down.</p>}

          <div className="ha-chosen" role="status">
            {chosen.length === 0 ? (
              <span className="muted small-14">Tick one thing, or several to put them together in one widget (up to {MOST}).</span>
            ) : (
              <>
                <span className="small-14">
                  <strong>{chosen.length === 1 ? chosen[0].name : `${chosen.length} things`}</strong>
                  {chosen.length > 1 && <span className="muted"> · {chosen.map((entity) => entity.name).join(", ")}</span>}
                </span>
                {chosen.length > 1 && (
                  <label className="ha-title">
                    <span className="muted small">Widget title</span>
                    <input className="parts-text" type="text" maxLength={60} value={title} onChange={(event) => setTitle(event.target.value)} />
                  </label>
                )}
                <span className="header-actions">
                  <Button size="bar" kind="primary" disabled={busy} onClick={() => void run(() => onMake(chosen, title))}>
                    {chosen.length === 1 ? "Make a widget" : "Make one widget with them all"}
                  </Button>
                  {addTo && (
                    <Button size="bar" disabled={busy} onClick={() => void run(() => addTo.add(chosen))}>
                      Add to “{addTo.name}”
                    </Button>
                  )}
                  <Button size="bar" disabled={busy} onClick={() => setPicked([])}>
                    Clear
                  </Button>
                </span>
              </>
            )}
          </div>
          <p className="muted small">
            {addTo ? "" : "To add to a widget you already have, select it in the picture first. "}
            In any part's text you can also write {"{ha.light.desk}"} for a state, {"{ha.light.desk.name}"} for a name, or any attribute such as {"{ha.light.desk.brightness}"}.
          </p>
        </>
      )}
    </section>
  );
}
