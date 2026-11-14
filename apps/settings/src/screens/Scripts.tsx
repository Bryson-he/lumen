// Scripted widgets: what each script asks to be allowed to do, its text for
// reading, and the buttons that let it run or stop it. A script never runs
// until it has been allowed here, and is asked about again if it changes.

import { useEffect, useState } from "react";
import { useLumen } from "../store";
import type { ScriptInfo } from "../types";
import { Button } from "../ui";

/** What a script asks for, in plain words. */
function asks(script: ScriptInfo): string[] {
  const list: string[] = [];
  if (script.permissions.files) list.push("Read and change files on this PC");
  if (script.permissions.network) list.push("Go online");
  if (script.permissions.run) list.push("Run and open programs");
  return list;
}

/** One script: its state, what it asks for, and the choice. */
export function ScriptCard({ script }: { script: ScriptInfo }) {
  const { request } = useLumen();
  const [text, setText] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const wanted = asks(script);

  // Load the text when it is asked for, and again if the script changes while it is open.
  useEffect(() => {
    if (!reading) return;
    let current = true;
    void request<{ text: string }>("readScript", { file: script.file }).then((result) => {
      if (current) setText(result ? result.text : null);
    });
    return () => {
      current = false;
    };
  }, [reading, script.file, script.hash, request]);

  const state = script.missing
    ? `The file ${script.script} is not in the widgets folder, so there is nothing to run.`
    : script.approved
      ? script.error
        ? `Allowed, but it stopped with a mistake: ${script.error}`
        : "Allowed and running while its widget is on screen."
      : script.changed
        ? "This script, or what it asks for, has changed since you allowed it. It is stopped until you allow it again."
        : "This script has not been allowed, so it is not running.";

  return (
    <section className={`script-card${script.approved ? "" : " script-card-waiting"}`} aria-label={`Script for ${script.name}`}>
      <div className="field-row wrap">
        <strong>
          {script.name} <span className="muted">· {script.script}</span>
        </strong>
        {!script.missing && (
          <span className="header-actions">
            <Button size="bar" aria-expanded={reading} onClick={() => setReading((open) => !open)}>
              {reading ? "Hide the script" : "Read the script"}
            </Button>
            {script.approved ? (
              <Button size="bar" onClick={() => void request("revokeScript", { file: script.file })}>
                Stop allowing
              </Button>
            ) : (
              <Button size="bar" kind="primary" onClick={() => void request("approveScript", { file: script.file, hash: script.hash })}>
                {script.changed ? "Allow it again" : "Allow it to run"}
              </Button>
            )}
          </span>
        )}
      </div>
      <p className={`small-13${script.approved && script.error ? " script-problem" : " muted"}`} role={script.approved && script.error ? "alert" : undefined}>
        {state}
      </p>
      {!script.missing &&
        (wanted.length > 0 ? (
          <>
            <p className="small-13">{script.approved ? "It is allowed to:" : "If you allow it, it will be able to:"}</p>
            <ul className="script-asks small-13">
              {wanted.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
            {!script.approved && <p className="muted small">Only allow a script from someone you trust. With these, a script can do the same things on this PC that you can.</p>}
          </>
        ) : (
          <p className="small-13">It asks for nothing beyond Lumen's own live values: it cannot touch files, go online or run programs.</p>
        ))}
      {reading && (
        <pre className="script-text" tabIndex={0} aria-label={`Text of ${script.script}`}>
          {text ?? "Reading…"}
        </pre>
      )}
    </section>
  );
}

/** Every script that is waiting to be allowed, for the top of the Widgets screen. */
export function ScriptsWaiting() {
  const { scripts } = useLumen();
  const waiting = scripts.filter((script) => !script.approved && !script.missing);
  if (waiting.length === 0) return null;
  return (
    <section className="panel scripts-waiting" aria-label="Scripts waiting to be allowed">
      <h2 className="panel-title">{waiting.length === 1 ? "A widget's script is waiting for your OK" : `${waiting.length} widget scripts are waiting for your OK`}</h2>
      {waiting.map((script) => (
        <ScriptCard key={script.file} script={script} />
      ))}
    </section>
  );
}
