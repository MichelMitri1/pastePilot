import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import DebugApp from "./debug/DebugApp";
import ReviewApp from "./review/ReviewApp";
import { Bar, Hud } from "./overlay/Overlay";
import "./styles.css";

// One bundle, several windows: "settings", "debug" (GitHub Debug), "review" (Assignment Review),
// and on Windows the "hud" status pill and the rewrite "bar".
const label = getCurrentWindow().label;
const overlays: Record<string, typeof Hud> = { hud: Hud, bar: Bar };
if (overlays[label]) document.documentElement.classList.add("overlay");
const Root = overlays[label] ?? (label === "debug" ? DebugApp : label === "review" ? ReviewApp : App);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
