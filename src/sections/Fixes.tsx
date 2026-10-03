import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "../api";
import type { Fix } from "../api";
import { ConfirmButton, FileTextButton } from "../components/common";
import type { SectionProps } from "./types";

const EMPTY: Fix = { id: null, title: "", problem: "", solution: "", snippet: "", tags: "", projectType: "" };

const IMPORT_HINT = `Title: Import casing breaks on Netlify
Problem: Works locally, "Module not found" after deploying
Solution: Make the import path match the file name's exact casing
Tags: imports, deploy
Project: react
---
Title: ...

Or a JSON array: [{"title": "...", "problem": "...", "solution": "...", "snippet": "...", "tags": "..."}]`;

export default function Fixes({ flash }: SectionProps) {
  const [items, setItems] = useState<Fix[]>([]);
  const [editing, setEditing] = useState<Fix | null>(null);
  const [filter, setFilter] = useState("");
  const [importText, setImportText] = useState<string | null>(null);

  const load = useCallback(() => api.listFixes().then(setItems).catch((e) => flash(String(e), true)), [flash]);
  useEffect(() => {
    load();
  }, [load]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? items.filter((f) => `${f.title} ${f.problem} ${f.solution} ${f.tags} ${f.projectType}`.toLowerCase().includes(q)) : items;
  }, [items, filter]);

  const save = async () => {
    if (!editing) return;
    try {
      await api.saveFix(editing);
      setEditing(null);
      await load();
      flash("Fix saved");
    } catch (e) {
      flash(String(e), true);
    }
  };

  const runImport = async (text: string) => {
    try {
      const n = await api.importFixes(text);
      setImportText(null);
      await load();
      flash(`Imported ${n} fix${n === 1 ? "" : "es"}`);
    } catch (e) {
      flash(String(e), true);
    }
  };

  return (
    <>
      <p className="muted small">
        Reusable fixes for problems you see often. GitHub Debug looks up the most relevant ones for each case and tells
        you when one applies. Save one from a diagnosis with “Save as fix”.
      </p>
      {editing ? (
        <section className="card">
          <label>Title</label>
          <input value={editing.title} onChange={(e) => setEditing({ ...editing, title: e.target.value })} />
          <label>Problem / symptoms</label>
          <textarea rows={2} value={editing.problem} onChange={(e) => setEditing({ ...editing, problem: e.target.value })} />
          <label>Solution</label>
          <textarea rows={3} value={editing.solution} onChange={(e) => setEditing({ ...editing, solution: e.target.value })} />
          <label>Code snippet <span className="muted">(optional)</span></label>
          <textarea rows={3} className="mono-input" value={editing.snippet} onChange={(e) => setEditing({ ...editing, snippet: e.target.value })} />
          <div className="grid">
            <div className="field">
              <label>Tags</label>
              <input placeholder="imports, css…" value={editing.tags} onChange={(e) => setEditing({ ...editing, tags: e.target.value })} />
            </div>
            <div className="field">
              <label>Project type</label>
              <input placeholder="react, next, vanilla…" value={editing.projectType} onChange={(e) => setEditing({ ...editing, projectType: e.target.value })} />
            </div>
          </div>
          <div className="row end">
            <button onClick={() => setEditing(null)}>Cancel</button>
            <button className="primary" onClick={save}>{editing.id ? "Save changes" : "Add fix"}</button>
          </div>
        </section>
      ) : importText !== null ? (
        <section className="card">
          <label>Import fixes</label>
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
          <button className="primary" onClick={() => setEditing({ ...EMPTY })}>Add fix</button>
          <button onClick={() => setImportText("")}>Import…</button>
          <input className="grow" placeholder={`Search ${items.length} fixes`} value={filter} onChange={(e) => setFilter(e.target.value)} />
        </section>
      )}
      <ul className="list">
        {shown.map((f) => (
          <li key={f.id ?? 0}>
            <div className="grow clip">
              <div className="clip"><strong>{f.title}</strong></div>
              <div className="clip muted">{f.solution}</div>
            </div>
            {!!f.uses && <span className="tag">used {f.uses}×</span>}
            {f.projectType && <span className="tag">{f.projectType}</span>}
            <button className="link" onClick={() => setEditing({ ...f })}>Edit</button>
            <button className="link danger-text" onClick={() => api.deleteFix(f.id!).then(load)}>Delete</button>
          </li>
        ))}
        {items.length === 0 && <li className="muted">No saved fixes yet.</li>}
      </ul>
      {items.length > 0 && (
        <section className="row end">
          <ConfirmButton label="Delete all fixes" onConfirm={() => api.clearFixes().then(load).then(() => flash("Fix library cleared"))} />
        </section>
      )}
    </>
  );
}
