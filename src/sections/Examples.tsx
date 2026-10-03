import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "../api";
import { MODES } from "../api";
import type { ReplyExample } from "../api";
import { ConfirmButton, FileTextButton, NumberField, Toggle } from "../components/common";
import type { SectionProps } from "./types";

const EMPTY: ReplyExample = { id: null, studentMessage: "", reply: "", category: "" };

const IMPORT_HINT = `Student: My npm install fails with EACCES
Me: Hey! Try running it without sudo first :)
Category: technical
---
Student: ...
Me: ...

Or a JSON array: [{"student": "...", "reply": "...", "category": "..."}]`;

export default function Examples({ settings, update, flash }: SectionProps) {
  const [items, setItems] = useState<ReplyExample[]>([]);
  const [editing, setEditing] = useState<ReplyExample | null>(null);
  const [filter, setFilter] = useState("");
  const [importText, setImportText] = useState<string | null>(null);

  const load = useCallback(() => api.listExamples().then(setItems).catch((e) => flash(String(e), true)), [flash]);
  useEffect(() => {
    load();
  }, [load]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return items;
    return items.filter((e) => `${e.studentMessage} ${e.reply} ${e.category}`.toLowerCase().includes(q));
  }, [items, filter]);

  const save = async () => {
    if (!editing) return;
    try {
      await api.saveExample(editing);
      setEditing(null);
      await load();
      flash("Example saved");
    } catch (e) {
      flash(String(e), true);
    }
  };

  const runImport = async (text: string) => {
    try {
      const n = await api.importExamples(text);
      setImportText(null);
      await load();
      flash(`Imported ${n} example${n === 1 ? "" : "s"}`);
    } catch (e) {
      flash(String(e), true);
    }
  };

  return (
    <>
      <section className="row wrap">
        <Toggle checked={settings.examplesEnabled} onChange={(v) => update("examplesEnabled", v)}>
          Use my past replies to match my style
        </Toggle>
        <NumberField label="Examples per reply" value={settings.maxExamples} min={0} max={8} onChange={(v) => update("maxExamples", v)} />
      </section>
      <p className="muted small">
        Only the few examples most similar to the current message are sent. Tip: click ★ Save on the rewrite bar after
        editing a reply, and it gets added here.
      </p>

      {editing ? (
        <section className="card">
          <label>Student message</label>
          <textarea rows={3} value={editing.studentMessage} onChange={(e) => setEditing({ ...editing, studentMessage: e.target.value })} />
          <label>My reply</label>
          <textarea rows={4} value={editing.reply} onChange={(e) => setEditing({ ...editing, reply: e.target.value })} />
          <label>Category <span className="muted">(optional)</span></label>
          <input list="example-categories" value={editing.category} onChange={(e) => setEditing({ ...editing, category: e.target.value })} />
          <datalist id="example-categories">{MODES.map((m) => <option key={m.id} value={m.id} />)}</datalist>
          <div className="row end">
            <button onClick={() => setEditing(null)}>Cancel</button>
            <button className="primary" onClick={save}>{editing.id ? "Save changes" : "Add example"}</button>
          </div>
        </section>
      ) : importText !== null ? (
        <section className="card">
          <label>Import examples</label>
          <textarea rows={8} placeholder={IMPORT_HINT} value={importText} onChange={(e) => setImportText(e.target.value)} />
          <div className="row spread">
            <FileTextButton onText={setImportText} />
            <div className="row">
              <button onClick={() => setImportText(null)}>Cancel</button>
              <button className="primary" disabled={!importText.trim()} onClick={() => runImport(importText)}>Import</button>
            </div>
          </div>
        </section>
      ) : (
        <section className="row">
          <button className="primary" onClick={() => setEditing({ ...EMPTY })}>Add example</button>
          <button onClick={() => setImportText("")}>Import…</button>
          <input className="grow" placeholder={`Search ${items.length} examples`} value={filter} onChange={(e) => setFilter(e.target.value)} />
        </section>
      )}

      <ul className="list">
        {shown.map((e) => (
          <li key={e.id ?? 0}>
            <div className="grow clip">
              <div className="clip"><span className="muted">Student:</span> {e.studentMessage}</div>
              <div className="clip"><span className="muted">Me:</span> {e.reply}</div>
            </div>
            {e.category && <span className="tag">{e.category}</span>}
            <button className="link" onClick={() => setEditing({ ...e })}>Edit</button>
            <button className="link danger-text" onClick={() => api.deleteExample(e.id!).then(load)}>Delete</button>
          </li>
        ))}
        {items.length === 0 && <li className="muted">No examples yet.</li>}
      </ul>

      {items.length > 0 && (
        <section className="row end">
          <ConfirmButton label="Delete all examples" onConfirm={() => api.clearExamples().then(load).then(() => flash("All examples deleted"))} />
        </section>
      )}
    </>
  );
}
