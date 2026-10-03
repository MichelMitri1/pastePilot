import { ShortcutInput, Toggle } from "../components/common";
import type { SectionProps } from "./types";

const REWRITES = [
  ["⌥1", "Shorter"],
  ["⌥2", "Friendlier"],
  ["⌥3", "More professional"],
  ["⌥4", "Explain more"],
  ["⌥5", "Regenerate"],
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
      </section>
      <p className="muted small">Click a field and press the new key combination. Changes apply when you click Save.</p>

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
