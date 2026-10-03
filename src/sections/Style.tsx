import { useCallback, useEffect, useState } from "react";
import * as api from "../api";
import { ConfirmButton, Toggle } from "../components/common";
import type { SectionProps } from "./types";

export default function Style({ settings, update, status, flash }: SectionProps) {
  const [learning, setLearning] = useState<{ samples: number; profile: string | null } | null>(null);
  const load = useCallback(() => api.getLearning().then(setLearning), []);
  useEffect(() => {
    load();
  }, [load]);

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
        <textarea rows={4} value={settings.customInstructions} onChange={(e) => update("customInstructions", e.target.value)} />
        <span className="muted small">For facts that should only be used when relevant, use the Knowledge Base instead.</span>
      </section>

      <section className="toggles">
        <Toggle checked={settings.removeFluff} onChange={(v) => update("removeFluff", v)}>
          Remove AI fluff before pasting <span className="muted small">(“Certainly!”, “It appears that…”, “I hope this helps!”)</span>
        </Toggle>
      </section>

      <section className="card">
        <label>Learning from your edits</label>
        <Toggle checked={settings.feedbackLearning} onChange={(v) => update("feedbackLearning", v)}>
          When I change a reply a lot before sending, learn from it
        </Toggle>
        <Toggle checked={settings.feedbackSaveExamples} onChange={(v) => update("feedbackSaveExamples", v)}>
          Save heavily edited replies as reply examples
        </Toggle>
        {learning && (
          <>
            <span className="muted small">
              {learning.samples === 0
                ? "Nothing learned yet. PastePilot compares what you send with its draft (it sees the reply box empty when you send)."
                : `Learned from ${learning.samples} edited repl${learning.samples === 1 ? "y" : "ies"}.`}
            </span>
            {learning.profile ? (
              <pre className="learned">{learning.profile}</pre>
            ) : (
              learning.samples > 0 && <span className="muted small">No consistent pattern yet. Rules appear after a few similar edits.</span>
            )}
            {learning.samples > 0 && (
              <div className="row end">
                <ConfirmButton label="Forget what was learned" onConfirm={() => api.resetLearning().then(load).then(() => flash("Learned rules cleared"))} />
              </div>
            )}
          </>
        )}
      </section>
    </>
  );
}
