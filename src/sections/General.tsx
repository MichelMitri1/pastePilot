import { useState } from "react";
import * as api from "../api";
import { Toggle } from "../components/common";
import Updates from "../components/Updates";
import type { SectionProps } from "./types";

export default function General({ settings, update, status, refreshStatus, flash }: SectionProps) {
  const [keyDraft, setKeyDraft] = useState("");

  const saveKey = async () => {
    try {
      await api.setApiKey(keyDraft.trim());
      setKeyDraft("");
      await refreshStatus();
      flash("API key saved to Keychain");
    } catch (e) {
      flash(String(e), true);
    }
  };

  return (
    <>
      {!status.accessibilityTrusted && (
        <div className="banner">
          <strong>Accessibility permission needed.</strong> Open System Settings → Privacy &amp; Security →
          Accessibility and turn on PastePilot.
          <div className="row">
            <button onClick={() => api.requestAccessibility().then(refreshStatus)}>Request access</button>
            <button onClick={api.openAccessibilitySettings}>Open System Settings</button>
          </div>
        </div>
      )}
      {status.dbError && <div className="banner">{status.dbError}</div>}

      <section>
        <label>OpenAI API key</label>
        {status.hasApiKey && (
          <div className="row">
            <span className="ok">Stored in Keychain</span>
            <button className="link" onClick={() => api.clearApiKey().then(refreshStatus)}>Remove</button>
          </div>
        )}
        <div className="row">
          <input
            type="password"
            placeholder={status.hasApiKey ? "Replace key…" : "sk-…"}
            value={keyDraft}
            onChange={(e) => setKeyDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && keyDraft && saveKey()}
          />
          <button disabled={!keyDraft.trim()} onClick={saveKey}>Save key</button>
        </div>
      </section>

      <section className="toggles">
        <Toggle checked={settings.autoPaste} onChange={(v) => update("autoPaste", v)}>
          Auto-paste reply (never presses Enter)
        </Toggle>
        <Toggle checked={settings.showHud} onChange={(v) => update("showHud", v)}>
          Show floating status indicator
        </Toggle>
        <Toggle checked={settings.rewriteBar} onChange={(v) => update("rewriteBar", v)}>
          Show rewrite bar after each reply
        </Toggle>
        <Toggle checked={settings.launchAtLogin} onChange={(v) => update("launchAtLogin", v)}>
          Launch at login
        </Toggle>
      </section>

      <Updates />
    </>
  );
}
