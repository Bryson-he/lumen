// Updates from Lumen's GitHub releases. Lumen looks for a newer version at
// most once a day when the app is opened (this can be switched off on the
// Settings screen), and only says one is there: nothing is downloaded or
// installed until the user clicks. The download is checked against Lumen's
// own signature before it is run.

import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Button } from "./ui";

const AUTO_KEY = "lumen.updates.auto";
const LAST_KEY = "lumen.updates.lastCheck";
const DAY_MS = 24 * 60 * 60 * 1000;

function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not being able to remember only means checking again next time.
  }
}

type Phase = "idle" | "checking" | "current" | "available" | "installing" | "failed";

interface Updates {
  phase: Phase;
  /** The newer version, while one is on offer. */
  version: string | null;
  notes: string | null;
  /** Why the last check or install failed. */
  problem: string | null;
  /** Whether Lumen looks for updates by itself when it is opened. */
  auto: boolean;
  setAuto: (on: boolean) => void;
  checkNow: () => Promise<void>;
  install: () => Promise<void>;
  later: () => void;
}

const UpdatesContext = createContext<Updates | null>(null);

export function UpdatesProvider({ children }: { children: ReactNode }) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [problem, setProblem] = useState<string | null>(null);
  const [auto, setAutoState] = useState(() => stored(AUTO_KEY) !== "off");
  const found = useRef<Update | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [notes, setNotes] = useState<string | null>(null);

  const look = useCallback(async (quiet: boolean) => {
    setPhase("checking");
    setProblem(null);
    try {
      const update = await check();
      store(LAST_KEY, String(Date.now()));
      found.current = update;
      setVersion(update?.version ?? null);
      setNotes(update?.body?.trim() || null);
      setPhase(update ? "available" : quiet ? "idle" : "current");
    } catch (error) {
      // A check that fails by itself (no internet) is not worth interrupting anyone for.
      setProblem(quiet ? null : `Could not check for updates: ${String(error)}`);
      setPhase(quiet ? "idle" : "failed");
    }
  }, []);

  // Once a day at most, when the app is opened.
  useEffect(() => {
    if (!auto) return;
    const last = Number(stored(LAST_KEY) ?? 0);
    if (Date.now() - last < DAY_MS) return;
    const timer = setTimeout(() => void look(true), 4000);
    return () => clearTimeout(timer);
  }, [auto, look]);

  const value = useMemo<Updates>(
    () => ({
      phase,
      version,
      notes,
      problem,
      auto,
      setAuto: (on) => {
        store(AUTO_KEY, on ? "on" : "off");
        setAutoState(on);
      },
      checkNow: () => look(false),
      install: async () => {
        const update = found.current;
        if (!update) return;
        setPhase("installing");
        setProblem(null);
        try {
          await update.downloadAndInstall();
          await relaunch();
        } catch (error) {
          setProblem(`The update could not be installed: ${String(error)}`);
          setPhase("failed");
        }
      },
      later: () => setPhase("idle"),
    }),
    [phase, version, notes, problem, auto, look],
  );
  return <UpdatesContext.Provider value={value}>{children}</UpdatesContext.Provider>;
}

export function useUpdates(): Updates {
  const updates = useContext(UpdatesContext);
  if (!updates) throw new Error("useUpdates outside UpdatesProvider");
  return updates;
}

/** The strip at the top of the app while a newer version is on offer. */
export function UpdateStrip() {
  const { phase, version, install, later } = useUpdates();
  if (phase !== "available" && phase !== "installing") return null;
  return (
    <div className="strip update-strip" role="status">
      <span>{phase === "installing" ? `Downloading Lumen ${version}… Lumen will close and reopen.` : `Lumen ${version} is available.`}</span>
      {phase === "available" && (
        <span className="header-actions">
          <Button size="bar" kind="primary" onClick={() => void install()}>
            Update now
          </Button>
          <Button size="bar" onClick={later}>
            Later
          </Button>
        </span>
      )}
    </div>
  );
}
