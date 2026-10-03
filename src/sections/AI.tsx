import { MODES } from "../api";
import type { ModeId, Settings } from "../api";
import type { SectionProps } from "./types";

// Fast, non-reasoning models first. Any model id can be typed in.
const MODELS = ["gpt-4.1-mini", "gpt-4.1-nano", "gpt-4o-mini", "gpt-5-mini", "gpt-5-nano", "gpt-4.1"];

export default function AI({ settings, update, status }: SectionProps) {
  const setInstruction = (id: ModeId, text: string) =>
    update("modeInstructions", { ...settings.modeInstructions, [id]: text });

  return (
    <>
      <section className="grid">
        <div className="field">
          <label>Model</label>
          <input list="models" value={settings.model} onChange={(e) => update("model", e.target.value)} />
          <datalist id="models">{MODELS.map((m) => <option key={m} value={m} />)}</datalist>
          <span className="muted small">gpt-4.1-mini is the fastest model that still sounds natural.</span>
        </div>
        <div className="field">
          <label>Mode</label>
          <select
            value={settings.modeOverride}
            onChange={(e) => update("modeOverride", e.target.value as Settings["modeOverride"])}
          >
            <option value="auto">Auto-detect</option>
            {MODES.map((m) => <option key={m.id} value={m.id}>Always {m.label}</option>)}
          </select>
          <span className="muted small">Detection is local and instant. Override per reply from the rewrite bar.</span>
        </div>
      </section>

      <section className="field">
        <label>GitHub Debug model</label>
        <input list="models" value={settings.debugModel} onChange={(e) => update("debugModel", e.target.value)} />
        <span className="muted small">
          Used to read code in GitHub Debug Mode. gpt-4.1-mini is fast; gpt-4.1 is slower but catches subtler bugs.
          Replies always use the main model and your writing style.
        </span>
        <label className="check">
          <input type="checkbox" checked={settings.debugCompareCommits} onChange={(e) => update("debugCompareCommits", e.target.checked)} />
          <span>Always compare recent commits <span className="muted small">(otherwise only when the student says it broke after a change; uses 4 GitHub requests)</span></span>
        </label>
      </section>

      <h3>Mode instructions</h3>
      {MODES.map((m) => (
        <section key={m.id}>
          <div className="row spread">
            <label>{m.label}</label>
            <button className="link" onClick={() => setInstruction(m.id, status.defaultModeInstructions[m.id])}>
              Reset
            </button>
          </div>
          <textarea
            rows={3}
            value={settings.modeInstructions[m.id]}
            onChange={(e) => setInstruction(m.id, e.target.value)}
          />
        </section>
      ))}
    </>
  );
}
