import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "../api";
import type { KbEntry } from "../api";
import { ConfirmButton, FileTextButton, NumberField, Toggle } from "../components/common";
import type { SectionProps } from "./types";

const EMPTY: KbEntry = { id: null, title: "", content: "", category: "", tags: "", enabled: true };

const IMPORT_HINT = `# Refund policy
Category: billing
Tags: refund, money back

Full refund within 14 days of purchase if less than 20% of the course is completed.

# Reset password
Use the "Forgot password" link on the login page.

Or a JSON array: [{"title": "...", "content": "...", "category": "...", "tags": "a, b"}]`;

export default function Knowledge({ settings, update, flash }: SectionProps) {
  const [items, setItems] = useState<KbEntry[]>([]);
  const [editing, setEditing] = useState<KbEntry | null>(null);
  const [filter, setFilter] = useState("");
  const [importText, setImportText] = useState<string | null>(null);

  const load = useCallback(() => api.listKb().then(setItems).catch((e) => flash(String(e), true)), [flash]);
  useEffect(() => {
    load();
  }, [load]);

  const categories = useMemo(() => [...new Set(items.map((i) => i.category).filter(Boolean))], [items]);
  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return items;
    return items.filter((e) => `${e.title} ${e.content} ${e.category} ${e.tags}`.toLowerCase().includes(q));
  }, [items, filter]);

  const save = async () => {
    if (!editing) return;
    try {
      await api.saveKb(editing);
      setEditing(null);
      await load();
      flash("Entry saved");
    } catch (e) {
      flash(String(e), true);
    }
  };

  const runImport = async (text: string) => {
    try {
      const n = await api.importKb(text);
      setImportText(null);
      await load();
      flash(`Imported ${n} entr${n === 1 ? "y" : "ies"}`);
    } catch (e) {
      flash(String(e), true);
    }
  };

  return (
    <>
      <section className="row wrap">
        <Toggle checked={settings.kbEnabled} onChange={(v) => update("kbEnabled", v)}>
          Use the knowledge base
        </Toggle>
        <NumberField label="Entries per reply" value={settings.maxKbEntries} min={0} max={8} onChange={(v) => update("maxKbEntries", v)} />
      </section>
      <p className="muted small">
        Trusted facts. Only entries that match the student's message are sent, and the AI is told never to state policies
        that aren't here. For billing questions it may only use these entries.
      </p>

      {editing ? (
        <section className="card">
          <label>Title</label>
          <input value={editing.title} onChange={(e) => setEditing({ ...editing, title: e.target.value })} />
          <label>Content</label>
          <textarea rows={6} value={editing.content} onChange={(e) => setEditing({ ...editing, content: e.target.value })} />
          <div className="grid">
            <div className="field">
              <label>Category</label>
              <input list="kb-categories" placeholder="billing, course, access…" value={editing.category} onChange={(e) => setEditing({ ...editing, category: e.target.value })} />
              <datalist id="kb-categories">
                {[...new Set(["billing", "technical", "career", "course", "access", "policy", ...categories])].map((c) => <option key={c} value={c} />)}
              </datalist>
            </div>
            <div className="field">
              <label>Tags <span className="muted">(comma separated)</span></label>
              <input value={editing.tags} onChange={(e) => setEditing({ ...editing, tags: e.target.value })} />
            </div>
          </div>
          <Toggle checked={editing.enabled} onChange={(v) => setEditing({ ...editing, enabled: v })}>Enabled</Toggle>
          <div className="row end">
            <button onClick={() => setEditing(null)}>Cancel</button>
            <button className="primary" onClick={save}>{editing.id ? "Save changes" : "Add entry"}</button>
          </div>
        </section>
      ) : importText !== null ? (
        <section className="card">
          <label>Import entries</label>
          <textarea rows={9} placeholder={IMPORT_HINT} value={importText} onChange={(e) => setImportText(e.target.value)} />
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
          <button className="primary" onClick={() => setEditing({ ...EMPTY })}>Add entry</button>
          <button onClick={() => setImportText("")}>Import…</button>
          <input className="grow" placeholder={`Search ${items.length} entries`} value={filter} onChange={(e) => setFilter(e.target.value)} />
        </section>
      )}

      <ul className="list">
        {shown.map((e) => (
          <li key={e.id ?? 0} className={e.enabled ? "" : "disabled"}>
            <input
              type="checkbox"
              title={e.enabled ? "Enabled" : "Disabled"}
              checked={e.enabled}
              onChange={(ev) => api.setKbEnabled(e.id!, ev.target.checked).then(load)}
            />
            <div className="grow clip">
              <div className="clip"><strong>{e.title}</strong></div>
              <div className="clip muted">{e.content}</div>
            </div>
            {e.category && <span className="tag">{e.category}</span>}
            <button className="link" onClick={() => setEditing({ ...e })}>Edit</button>
            <button className="link danger-text" onClick={() => api.deleteKb(e.id!).then(load)}>Delete</button>
          </li>
        ))}
        {items.length === 0 && <li className="muted">No entries yet.</li>}
      </ul>

      {items.length > 0 && (
        <section className="row end">
          <ConfirmButton label="Delete knowledge base" onConfirm={() => api.clearKb().then(load).then(() => flash("Knowledge base deleted"))} />
        </section>
      )}
    </>
  );
}
