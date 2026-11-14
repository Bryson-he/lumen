// Settings: how Lumen itself behaves. For now, whether it starts with
// Windows, and where it keeps its files.

import { useEffect, useState } from "react";
import { PageHeader } from "../Shell";
import { useLumen } from "../store";
import type { StartupState } from "../types";
import { Button, Check } from "../ui";
import { useUpdates } from "../updates";
import { HaConnection } from "./HomeAssistant";

export function SettingsScreen() {
  const { request, status } = useLumen();
  const [startup, setStartup] = useState<StartupState | null>(null);
  const updates = useUpdates();
  useEffect(() => {
    let current = true;
    void request<StartupState>("getStartup").then((state) => {
      if (current && state) setStartup(state);
    });
    return () => {
      current = false;
    };
  }, [request]);

  return (
    <>
      <PageHeader title="Settings" tip="how Lumen itself behaves" />
      <div className="settings-list">
        <section className="panel" aria-label="Start-up">
          <h2 className="panel-title">Start-up</h2>
          <Check checked={startup?.enabled ?? false} disabled={!startup?.available} onChange={(on) => void request<StartupState>("setStartup", { on }).then((state) => state && setStartup(state))}>
            Start Lumen when I sign in to Windows
          </Check>
          <p className="muted small-13">
            {startup && !startup.available
              ? "Not offered for this copy of Lumen, which is running on a test profile."
              : "Lumen starts quietly in the notification area, with your widgets and bar. Windows lists it under Task Manager, Startup apps, where it can also be switched off."}
          </p>
        </section>

        <HaConnection />

        <section className="panel" aria-label="Updates">
          <h2 className="panel-title">Updates</h2>
          <Check checked={updates.auto} onChange={updates.setAuto}>
            Look for a newer version when Lumen is opened
          </Check>
          <p className="muted small-13">At most once a day, from Lumen's page on GitHub. Lumen only tells you one is there; nothing is downloaded or installed until you say so.</p>
          <div className="field-row wrap settings-update">
            <span role="status" className={updates.phase === "failed" ? "script-problem" : undefined}>
              {updates.phase === "checking" && "Checking…"}
              {updates.phase === "current" && "You have the newest version."}
              {updates.phase === "available" && `Lumen ${updates.version} is available.`}
              {updates.phase === "installing" && `Downloading Lumen ${updates.version}…`}
              {updates.phase === "failed" && updates.problem}
            </span>
            <span className="header-actions">
              {updates.phase === "available" && (
                <Button size="bar" kind="primary" onClick={() => void updates.install()}>
                  Update now
                </Button>
              )}
              <Button size="bar" disabled={updates.phase === "checking" || updates.phase === "installing"} onClick={() => void updates.checkNow()}>
                Check now
              </Button>
            </span>
          </div>
          {updates.phase === "available" && updates.notes && <p className="small-13 settings-notes">{updates.notes}</p>}
        </section>

        <section className="panel" aria-label="About">
          <h2 className="panel-title">About</h2>
          <dl className="settings-facts small-14">
            <dt className="muted">Version</dt>
            <dd>{status?.version ?? "–"}</dd>
            <dt className="muted">Your Lumen files</dt>
            <dd className="settings-path">{status?.dataDir ?? "–"}</dd>
            <dt className="muted">Using now</dt>
            <dd>{status ? `${status.cpuPercent.toFixed(1)}% CPU · ${Math.round(status.memoryMb)} MB` : "–"}</dd>
          </dl>
        </section>
      </div>
    </>
  );
}
