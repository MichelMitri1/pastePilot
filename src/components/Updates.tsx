import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../api";
import type { UpdateInfo } from "../api";

type State =
  | { kind: "checking" }
  | { kind: "info"; info: UpdateInfo }
  | { kind: "installing"; percent: number | null }
  | { kind: "error"; message: string; info?: UpdateInfo };

/** Version + "Check for updates" + one-click install (download, verify, replace, restart). */
export default function Updates() {
  const [state, setState] = useState<State>({ kind: "checking" });

  const check = useCallback(async () => {
    setState({ kind: "checking" });
    try {
      setState({ kind: "info", info: await api.checkForUpdate() });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }, []);

  useEffect(() => {
    check();
    const subs = [
      listen("check-updates", () => check()),
      listen<{ downloaded: number; total: number | null }>("update-progress", (e) => {
        const { downloaded, total } = e.payload;
        setState({ kind: "installing", percent: total ? Math.min(100, Math.round((downloaded / total) * 100)) : null });
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, [check]);

  const install = async () => {
    setState({ kind: "installing", percent: null });
    try {
      await api.installUpdate(); // the app restarts when this succeeds
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  };

  return (
    <section className="card">
      <div className="row spread">
        <label>Updates</label>
        {state.kind === "info" && <span className="muted small">Version {state.info.current}</span>}
      </div>

      {state.kind === "checking" && <span className="muted">Checking for updates…</span>}

      {state.kind === "info" && !state.info.available && (
        <div className="row spread">
          <span className="ok">You're on the latest version.</span>
          <button onClick={check}>Check again</button>
        </div>
      )}

      {state.kind === "info" && state.info.available && (
        <>
          <strong>PastePilot {state.info.version} is available.</strong>
          {state.info.notes && <p className="muted small notes">{state.info.notes}</p>}
          <div className="row">
            <button className="primary" onClick={install}>Download and install</button>
            <span className="muted small">PastePilot restarts by itself when it's done.</span>
          </div>
        </>
      )}

      {state.kind === "installing" && (
        <div className="field">
          <span>{state.percent === null ? "Downloading…" : state.percent < 100 ? `Downloading… ${state.percent}%` : "Installing…"}</span>
          <progress max={100} value={state.percent ?? undefined} />
        </div>
      )}

      {state.kind === "error" && (
        <div className="row spread">
          <span className="error">{state.message}</span>
          <button onClick={check}>Try again</button>
        </div>
      )}
    </section>
  );
}
