import { prettyShortcut, ShortcutInput, Toggle } from "../components/common";
import type { SectionProps } from "./types";
import { ALT, MENU } from "../platform";

const REWRITES = [
  [`${ALT}1`, "Shorter"],
  [`${ALT}2`, "Friendlier"],
  [`${ALT}3`, "More professional"],
  [`${ALT}4`, "Explain more"],
  [`${ALT}5`, "Regenerate"],
];

export default function Shortcuts({ settings, update }: SectionProps) {
  return (
    <>
      <section className="grid">
        <div className="field">
          <label>Generate reply</label>
          <ShortcutInput value={settings.shortcut} onChange={(v) => update("shortcut", v)} />
        </div>
        <div className="field">
          <label>New conversation + generate</label>
          <div className="row">
            <ShortcutInput value={settings.newConversationShortcut} onChange={(v) => update("newConversationShortcut", v)} />
            <button className="link" onClick={() => update("newConversationShortcut", "")}>None</button>
          </div>
        </div>
        <div className="field">
          <label>Voice command (hold to talk)</label>
          <div className="row">
            <ShortcutInput value={settings.voiceShortcut} onChange={(v) => update("voiceShortcut", v)} />
          </div>
        </div>
        <div className="field">
          <label>Assignment Review</label>
          <div className="row">
            <ShortcutInput value={settings.reviewShortcut} onChange={(v) => update("reviewShortcut", v)} />
            <button className="link" onClick={() => update("reviewShortcut", "")}>None</button>
          </div>
        </div>
        <div className="field">
          <label>GitHub Debug</label>
          <div className="row">
            <ShortcutInput value={settings.debugShortcut} onChange={(v) => update("debugShortcut", v)} />
            <button className="link" onClick={() => update("debugShortcut", "")}>None</button>
          </div>
        </div>
      </section>
      <p className="muted small">Click a field and press the new key combination. Changes apply when you click Save.</p>

      <section className="card">
        <label>Multi-message cases</label>
        <span className="muted small">
          Select a student message and choose Add Selection to Case from the {MENU}; repeat for each message. The next {ALT}R
          or {ALT}G answers them as one case. Cases never carry over to a different ticket.
        </span>
      </section>

      <section className="card">
        <Toggle checked={settings.voiceEnabled} onChange={(v) => update("voiceEnabled", v)}>
          Voice commands
        </Toggle>
        <span className="muted small">
          Hold {prettyShortcut(settings.voiceShortcut)}, say a command, release. Try “reply to this”, “debug this repo”,
          “shorter”, “friendlier”, “more professional”, “explain more”, “regenerate”, “new conversation”, “add to case”,
          “save example”. Audio is recorded only while you hold the key and is sent to OpenAI for transcription. macOS asks
          for microphone access the first time.
        </span>
      </section>

      <section className="toggles">
        <Toggle checked={settings.rewriteShortcuts} onChange={(v) => update("rewriteShortcuts", v)}>
          Rewrite shortcuts while the rewrite bar is visible
        </Toggle>
      </section>
      <table className="keys">
        <tbody>
          {REWRITES.map(([key, label]) => (
            <tr key={key}><td className="kbd">{key}</td><td>{label}</td></tr>
          ))}
        </tbody>
      </table>
      <p className="muted small">
        These keys are only taken while the bar is on screen (30 seconds after a reply), so they don't interfere with
        typing the rest of the time.
      </p>
    </>
  );
}
