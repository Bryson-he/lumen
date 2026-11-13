// The code view: a widget file's YAML text, editable in the app. Saving it
// is the same as saving the file in any editor: the desktop reloads it.

import { useEffect, useRef, useState } from "react";
import { useLumen } from "../store";
import { Button } from "../ui";

export function CodeView({ file }: { file: string }) {
  const { request, widgetFiles } = useLumen();
  const [text, setText] = useState("");
  const [saved, setSaved] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const dirty = text !== saved;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  // Load the file, and load it again whenever it changes on disk (the designer
  // or another editor saved it), unless there are unsaved edits here.
  const info = widgetFiles.find((item) => item.file === file);
  const stamp = JSON.stringify([info?.parts, info?.size, info?.name, info?.error]);
  useEffect(() => {
    if (dirtyRef.current) return;
    let current = true;
    void request<{ text: string }>("readWidgetFile", { file }).then((result) => {
      if (current && result && !dirtyRef.current) {
        setText(result.text);
        setSaved(result.text);
      }
    });
    return () => {
      current = false;
    };
  }, [file, stamp, request]);

  const save = async () => {
    const result = await request<{ error: string | null; line: number | null }>("writeWidgetFile", { file, text });
    if (!result) return;
    setSaved(text);
    setNote(result.error ? null : "Saved. Your desktop is showing it.");
  };
  const revert = () => {
    setText(saved);
    setNote(null);
  };

  return (
    <section className="panel code-view" aria-label={`Text of ${file}`}>
      <div className="field-row wrap">
        <h2 className="panel-title">{file}</h2>
        <span className="header-actions">
          <Button size="bar" disabled={!dirty} onClick={revert}>
            Undo edits
          </Button>
          <Button size="bar" kind="primary" disabled={!dirty} onClick={() => void save()}>
            Save · Ctrl+S
          </Button>
        </span>
      </div>
      {info?.error ? (
        <div className="strip strip-danger" role="alert">
          <span>
            {info.errorLine ? `Line ${info.errorLine}: ` : ""}
            {info.error} Your desktop keeps showing the last version that worked.
          </span>
        </div>
      ) : (
        <p className="muted small-13">{dirty ? "Not saved yet." : (note ?? "This is the same file the designer edits. Saving from the designer rewrites it and drops comments.")}</p>
      )}
      <textarea
        className="code-text"
        spellCheck={false}
        aria-label={`YAML text of ${file}`}
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          setNote(null);
        }}
        onKeyDown={(event) => {
          if (event.ctrlKey && event.key.toLowerCase() === "s") {
            event.preventDefault();
            if (dirty) void save();
          }
          // Tab types two spaces instead of leaving the box (YAML is all about indentation).
          if (event.key === "Tab" && !event.shiftKey && !event.ctrlKey && !event.altKey) {
            event.preventDefault();
            const box = event.currentTarget;
            const { selectionStart: start, selectionEnd: end } = box;
            const next = `${text.slice(0, start)}  ${text.slice(end)}`;
            setText(next);
            requestAnimationFrame(() => box.setSelectionRange(start + 2, start + 2));
          }
        }}
      />
      <p className="muted small">
        Colours are theme roles (text, muted, highlight, primary, secondary, tertiary, neutral, surface, on-surface, outline, accent), never hex values. Live values go in braces: {"{clock.time}"}, {"{stats.cpu}"}, {"{media.title}"}.
      </p>
    </section>
  );
}
