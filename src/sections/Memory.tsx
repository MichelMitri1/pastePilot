import { useCallback, useEffect, useState } from "react";
import * as api from "../api";
import type { MemoryStatus } from "../api";
import { ConfirmButton, NumberField, Toggle } from "../components/common";
import type { SectionProps } from "./types";
import { ALT, MENU, SHIFT } from "../platform";

export default function Memory({ settings, update, flash }: SectionProps) {
  const [memory, setMemory] = useState<MemoryStatus | null>(null);
  const load = useCallback(() => api.getMemory().then(setMemory).catch((e) => flash(String(e), true)), [flash]);

  useEffect(() => {
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);

  const current = memory?.current;

  return (
    <>
      <section className="toggles">
        <Toggle checked={settings.memoryEnabled} onChange={(v) => update("memoryEnabled", v)}>
          Remember earlier messages in the same conversation
        </Toggle>
        <Toggle checked={settings.memoryAutoDetect} onChange={(v) => update("memoryAutoDetect", v)}>
          Detect conversations automatically (by app, page and window title)
        </Toggle>
        <span className="muted small">
          {settings.memoryAutoDetect
            ? "Each ticket or chat page gets its own context, so different students never mix."
            : `Manual session: everything goes into one conversation until you start a new one (${ALT}${SHIFT}R or the ${MENU}).`}
        </span>
      </section>

      <section className="grid three">
        <NumberField label="Max messages sent" value={settings.memoryMaxMessages} min={0} max={50} onChange={(v) => update("memoryMaxMessages", v)} />
        <NumberField label="Max characters sent" value={settings.memoryMaxChars} min={200} max={20000} onChange={(v) => update("memoryMaxChars", v)} />
        <NumberField label="Forget after (hours)" value={settings.memoryExpireHours} min={1} max={720} onChange={(v) => update("memoryExpireHours", v)} />
      </section>

      <section className="card">
        <div className="row spread">
          <label>Current conversation</label>
          <button className="link" onClick={load}>Refresh</button>
        </div>
        {current ? (
          <>
            <div className="muted small clip">{current.title}</div>
            <ul className="transcript">
              {current.messages.map((m) => (
                <li key={m.id} className={m.role}>
                  <span className="who">{m.role === "student" ? "Student" : "Me"}</span>
                  <span className="text">{m.content}</span>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <span className="muted">No active conversation. One starts with your next {ALT}R.</span>
        )}
        <div className="row">
          <button onClick={() => api.newConversation().then(load)}>New conversation</button>
          <button disabled={!current} onClick={() => api.clearCurrentConversation().then(load)}>Clear current conversation</button>
        </div>
      </section>

      <section className="row spread">
        <span className="muted small">
          Stored locally: {memory?.conversations ?? 0} conversations, {memory?.messages ?? 0} messages.
        </span>
        <ConfirmButton
          label="Delete all history"
          onConfirm={() => api.clearAllConversations().then(load).then(() => flash("Conversation history deleted"))}
        />
      </section>
    </>
  );
}
