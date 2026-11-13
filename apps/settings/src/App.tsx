import { useEffect } from "react";
import { ComingSoon } from "./screens/ComingSoon";
import { Home } from "./screens/Home";
import { Presets } from "./screens/Presets";
import { ThemeScreen } from "./screens/Theme";
import { Wallpapers } from "./screens/Wallpapers";
import { WidgetsScreen } from "./screens/Widgets";
import { Shell } from "./Shell";
import { LumenProvider, useLumen } from "./store";

export default function App() {
  return (
    <LumenProvider>
      <PencilAccent />
      <Shell>
        <CurrentScreen />
      </Shell>
    </LumenProvider>
  );
}

function CurrentScreen() {
  const { screen } = useLumen();
  switch (screen) {
    case "home":
      return <Home />;
    case "theme":
      return <ThemeScreen />;
    case "wallpapers":
      return <Wallpapers />;
    case "widgets":
      return <WidgetsScreen />;
    case "presets":
      return <Presets />;
    default:
      return <ComingSoon screen={screen} />;
  }
}

/**
 * The pencil accent follows the active look's primary colour. It only does so
 * when the theme's mode matches the app's (which follows Windows), because a
 * primary made for dark surfaces would be too pale on a light app, and vice versa.
 */
function PencilAccent() {
  const { shown } = useLumen();
  useEffect(() => {
    const dark = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const appMode = dark.matches ? "dark" : "light";
      const root = document.documentElement;
      if (shown && shown.mode === appMode) root.style.setProperty("--pencil", shown.roles.primary);
      else root.style.removeProperty("--pencil");
    };
    apply();
    dark.addEventListener("change", apply);
    return () => dark.removeEventListener("change", apply);
  }, [shown]);
  return null;
}
