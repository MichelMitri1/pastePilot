import type { SectionProps } from "./types";

export default function Style({ settings, update, status }: SectionProps) {
  return (
    <>
      <section>
        <div className="row spread">
          <label>Writing style</label>
          <button className="link" onClick={() => update("styleInstructions", status.defaultStyle)}>Reset to default</button>
        </div>
        <textarea rows={8} value={settings.styleInstructions} onChange={(e) => update("styleInstructions", e.target.value)} />
        <span className="muted small">
          Always applied on top: never invent facts, no promises about policies or refunds, ask for missing info, no em
          dashes, never mention AI. Saved reply examples also teach your length, punctuation and :) habits.
        </span>
      </section>
      <section>
        <label>Custom instructions <span className="muted">(optional, sent with every reply)</span></label>
        <textarea rows={5} value={settings.customInstructions} onChange={(e) => update("customInstructions", e.target.value)} />
        <span className="muted small">For facts that should only be used when relevant, use the Knowledge Base instead.</span>
      </section>
    </>
  );
}
