import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import DebugApp from "./debug/DebugApp";
import ReviewApp from "./review/ReviewApp";
import "./styles.css";

// One bundle, three windows: "settings", "debug" (GitHub Debug) and "review" (Assignment Review).
const label = getCurrentWindow().label;
const Root = label === "debug" ? DebugApp : label === "review" ? ReviewApp : App;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
