import React from "react";
import ReactDOM from "react-dom/client";
// Fonts are bundled with the app (no network): Inter, Space Grotesk, Caveat.
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/space-grotesk/500.css";
import "@fontsource/space-grotesk/600.css";
import "@fontsource/caveat/600.css";
import "@fontsource/caveat/700.css";
import "@lumen/tokens/tokens.css";
import "./styles.css";
import "./widgets.css";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
