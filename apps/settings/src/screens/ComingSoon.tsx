// Placeholder for the screens that arrive in later milestones.

import { PageHeader } from "../Shell";
import type { Screen } from "../store";
import { Tip } from "../ui";

const INFO: Partial<Record<Screen, { title: string; arrives: string; what: string }>> = {
  presets: { title: "Presets", arrives: "with presets (M7)", what: "Save a whole look, switch with one click, and export it as a .lumen file." },
  widgets: { title: "Widgets", arrives: "with the first widgets (M5 and M6)", what: "Place a clock, system stats and now playing on your desktop, and drag them around." },
  plugins: { title: "Plugins & editor", arrives: "with the creator tools", what: "Build your own widgets with a live preview." },
  gallery: { title: "Gallery", arrives: "with the creator tools", what: "Browse themes, presets and widgets made by other people." },
  settings: { title: "Settings", arrives: "soon", what: "Start with Windows, game mode, sketch notes, hotkeys and backups." },
};

export function ComingSoon({ screen }: { screen: Screen }) {
  const info = INFO[screen] ?? { title: "Coming soon", arrives: "later", what: "" };
  return (
    <>
      <PageHeader title={info.title} />
      <section className="card coming-soon">
        <p>{info.what}</p>
        <Tip accent size={22}>
          arrives {info.arrives}
        </Tip>
      </section>
    </>
  );
}
