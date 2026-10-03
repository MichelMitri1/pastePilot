import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import type { Settings, Status } from "./api";
import General from "./sections/General";
import AI from "./sections/AI";
import Style from "./sections/Style";
import Examples from "./sections/Examples";
import Knowledge from "./sections/Knowledge";
import Memory from "./sections/Memory";
import Shortcuts from "./sections/Shortcuts";
import type { SectionProps } from "./sections/types";

const SECTIONS: { id: string; label: string; component: (p: SectionProps) => React.ReactElement }[] = [
  { id: "general", label: "General", component: General },
  { id: "ai", label: "AI", component: AI },
  { id: "style", label: "Writing Style", component: Style },
  { id: "examples", label: "Reply Examples", component: Examples },
  { id: "knowledge", label: "Knowledge Base", component: Knowledge },
  { id: "memory", label: "Conversation Memory", component: Memory },
  { id: "shortcuts", label: "Shortcuts", component: Shortcuts },
];

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [section, setSection] = useState("general");
  const [message, setMessage] = useState<{ text: string; error?: boolean } | null>(null);

  const refreshStatus = useCallback(() => api.getStatus().then(setStatus), []);

  useEffect(() => {
    api.getSettings().then((s) => {
      setSettings(s);
      setSaved(s);
    });
    refreshStatus();
    // Accessibility is granted in System Settings, so re-check whenever we regain focus.
    window.addEventListener("focus", refreshStatus);
    const unlisten = listen<Settings>("settings-changed", (e) => {
      setSettings(e.payload);
      setSaved(e.payload);
    });
    // "Check for Updates…" in the menu bar: show the General page, where the Updates panel lives.
    const unlistenUpdates = listen("check-updates", () => setSection("general"));
    return () => {
      window.removeEventListener("focus", refreshStatus);
      unlisten.then((f) => f());
      unlistenUpdates.then((f) => f());
    };
  }, [refreshStatus]);

  const flash = useCallback((text: string, error = false) => {
    setMessage({ text, error });
    window.setTimeout(() => setMessage(null), error ? 4000 : 2500);
  }, []);

  const dirty = useMemo(() => JSON.stringify(settings) !== JSON.stringify(saved), [settings, saved]);

  if (!settings || !status) return null;

  const update: SectionProps["update"] = (k, v) => setSettings({ ...settings, [k]: v });

  const save = async () => {
    try {
      await api.saveSettings(settings);
      const fresh = await api.getSettings();
      setSettings(fresh);
      setSaved(fresh);
      flash("Saved");
    } catch (e) {
      flash(String(e), true);
    }
  };

  const Section = SECTIONS.find((s) => s.id === section)!.component;

  return (
    <div className="shell">
      <nav>
        {SECTIONS.map((s) => (
          <button key={s.id} className={s.id === section ? "active" : ""} onClick={() => setSection(s.id)}>
            {s.label}
          </button>
        ))}
      </nav>
      <div className="pane">
        <main>
          <h2>{SECTIONS.find((s) => s.id === section)!.label}</h2>
          <Section settings={settings} update={update} status={status} refreshStatus={refreshStatus} flash={flash} />
        </main>
        <footer className="row spread">
          <span className={message?.error ? "error" : message ? "ok" : "muted"}>
            {message?.text ?? (dirty ? "Unsaved changes" : "")}
          </span>
          <button className="primary" disabled={!dirty} onClick={save}>Save</button>
        </footer>
      </div>
    </div>
  );
}
