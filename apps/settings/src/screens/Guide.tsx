// "Make your look": a short guide on Home that walks from a fresh install to
// a saved preset, one step at a time, and says how saving works. Each step
// is ticked from what Lumen actually has, where that can be known.

import { useState } from "react";
import { useLumen, type Screen } from "../store";
import { Button, Tip } from "../ui";

const SEEN_KEY = "lumen.guide.seen";
const HIDDEN_KEY = "lumen.guide.hidden";

/** Small per-PC notes (which steps were opened, whether the guide is hidden). Works without storage too. */
function recall<T>(key: string, fallback: T): T {
  try {
    const text = localStorage.getItem(key);
    return text ? (JSON.parse(text) as T) : fallback;
  } catch {
    return fallback;
  }
}
function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Private windows and blocked storage: the guide still works, it just forgets.
  }
}

interface Step {
  id: string;
  title: string;
  about: string;
  done: boolean;
  actions: { label: string; screen: Screen; styles?: boolean }[];
}

export function Guide() {
  const { theme, styles, presets, go, setThemeView } = useLumen();
  const [seen, setSeen] = useState<string[]>(() => recall<string[]>(SEEN_KEY, []));
  const [hidden, setHidden] = useState(() => recall(HIDDEN_KEY, false));

  const styled = !!styles && (styles.styles.length > 0 || Object.values(styles.assigned).some((id) => id !== "default"));
  const steps: Step[] = [
    {
      id: "start",
      title: "Start from a wallpaper or a colour",
      about: "Lumen builds a full set of matching colours from it.",
      done: theme !== null,
      actions: [
        { label: "Pick a wallpaper", screen: "wallpapers" },
        { label: "Pick a colour", screen: "theme" },
      ],
    },
    {
      id: "colours",
      title: "Tune the colours",
      about: "Light or dark, how vivid, corner roundness and font. Changes preview on your desktop.",
      done: theme !== null && seen.includes("colours"),
      actions: [{ label: "Open Theme & colours", screen: "theme" }],
    },
    {
      id: "styles",
      title: "Shape the pieces",
      about: "Choose a ready-made style for widgets, bar, flyout and launcher, or duplicate one and make it yours.",
      done: styled,
      actions: [{ label: "Open component styles", screen: "theme", styles: true }],
    },
    {
      id: "widgets",
      title: "Arrange your widgets",
      about: "Add, move and resize them, or open the designer to change what is on one.",
      done: seen.includes("widgets"),
      actions: [{ label: "Open Widgets", screen: "widgets" }],
    },
    {
      id: "preset",
      title: "Save it as a preset",
      about: "A preset is a snapshot of the whole look. One click brings it back.",
      done: presets.length > 0,
      actions: [{ label: "Open Presets", screen: "presets" }],
    },
  ];
  const left = steps.filter((step) => !step.done).length;
  const next = steps.find((step) => !step.done)?.id ?? null;

  const hide = (value: boolean) => {
    setHidden(value);
    remember(HIDDEN_KEY, value);
  };
  const open = (step: Step, action: Step["actions"][number]) => {
    const now = seen.includes(step.id) ? seen : [...seen, step.id];
    setSeen(now);
    remember(SEEN_KEY, now);
    setThemeView(action.styles ? "styles" : "colours");
    go(action.screen);
  };

  if (hidden) {
    return (
      <button type="button" className="link small-14 lookguide-show" onClick={() => hide(false)}>
        Show the “make your look” guide
      </button>
    );
  }

  return (
    <section className="card lookguide" aria-label="Make your look, step by step">
      <div className="section-heading lookguide-heading">
        <h3>Make your look</h3>
        <Tip size={20} accent={left === 0}>
          {left === 0 ? "all done — it's yours" : left === 1 ? "one step to go" : `${left} steps to go`}
        </Tip>
        <button type="button" className="link small-13 lookguide-hide" onClick={() => hide(true)}>
          Hide this guide
        </button>
      </div>
      <ol className="lookguide-steps">
        {steps.map((step, index) => (
          <li key={step.id} className={`lookguide-step${step.done ? " lookguide-step-done" : ""}${step.id === next ? " lookguide-step-next" : ""}`}>
            <span className="lookguide-mark" aria-hidden="true">
              {step.done ? "✓" : index + 1}
            </span>
            <span className="lookguide-text">
              <span className="lookguide-title">
                {step.title}
                <span className="sr-only">{step.done ? " (done)" : ""}</span>
              </span>
              <span className="muted small-13">{step.about}</span>
            </span>
            <span className="lookguide-actions">
              {step.actions.map((action) => (
                <Button key={action.label} size="bar" kind={step.id === next ? "primary" : "outline"} onClick={() => open(step, action)}>
                  {action.label}
                </Button>
              ))}
            </span>
          </li>
        ))}
      </ol>
      <p className="muted small-13 lookguide-saving">
        <strong>How saving works.</strong> Colour changes are tried on your desktop for 15 seconds: press Keep, or they go back. Everything else (widgets and their parts, styles, the bar, the flyout, the launcher) is saved as you change it. A preset is a snapshot of all of
        it that you can return to with one click.
      </p>
    </section>
  );
}
