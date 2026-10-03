import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import DebugApp from "./debug/DebugApp";
import "./styles.css";

// One bundle, two windows: "settings" and "debug" (GitHub Debug Mode).
const isDebug = getCurrentWindow().label === "debug";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isDebug ? <DebugApp /> : <App />}</React.StrictMode>,
);
