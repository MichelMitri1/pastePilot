import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "../api";
import type { IssueRecord } from "../api";
import { ConfirmButton } from "../components/common";
import type { SectionProps } from "./types";

export default function Issues({ flash }: SectionProps) {
  const [items, setItems] = useState<IssueRecord[]>([]);
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState<number | null>(null);

  const load = useCallback(() => api.listIssues().then(setItems).catch((e) => flash(String(e), true)), [flash]);
  useEffect(() => {
    load();
  }, [load]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? items.filter((i) => `${i.repo} ${i.issue} ${i.summary} ${i.projectType}`.toLowerCase().includes(q)) : items;
  }, [items, filter]);

  const saveAsFix = async (i: IssueRecord) => {
    const findings: { cause?: string; fix?: string }[] = JSON.parse(i.findings || "[]");
    const first = findings[0] ?? {};
    try {
      await api.saveFix({
        title: (i.summary || first.cause || i.issue).slice(0, 80),
        problem: first.cause || i.issue,
        solution: first.fix || i.summary,
        snippet: "",
        tags: "",
        projectType: i.projectType,
      });
      flash("Saved to the fix library");
    } catch (e) {
      flash(String(e), true);
    }
  };

  return (
    <>
      <p className="muted small">
        Every GitHub Debug diagnosis is kept here locally, by repository and conversation. When a similar problem comes up
        again, it's shown as “Seen before” and given to the analysis as a hint.
      </p>
      <section className="row">
        <input className="grow" placeholder={`Search ${items.length} past cases`} value={filter} onChange={(e) => setFilter(e.target.value)} />
      </section>
      <ul className="list">
        {shown.map((i) => (
          <li key={i.id} className="column-item" onClick={() => setOpen(open === i.id ? null : i.id)}>
            <div className="row spread">
              <div className="grow clip">
                <strong>{i.repo || "Screenshot only"}</strong> <span className="muted small">· {api.timeAgo(i.createdAt)}</span>
                <div className="clip">{i.summary || i.issue}</div>
              </div>
              <span className={`tag ${i.status === "found" ? "" : "muted"}`}>{i.status === "found" ? i.confidence : "asked for info"}</span>
            </div>
            {open === i.id && (
              <div className="details" onClick={(e) => e.stopPropagation()}>
                <div><span className="muted">Student:</span> {i.issue}</div>
                {i.projectType && <div><span className="muted">Project:</span> {i.projectType}</div>}
                {i.reply && <div><span className="muted">Reply sent:</span> {i.reply}</div>}
                <div className="row end">
                  {i.status === "found" && <button onClick={() => saveAsFix(i)}>Save as fix</button>}
                  <button className="link danger-text" onClick={() => api.deleteIssue(i.id).then(load)}>Delete</button>
                </div>
              </div>
            )}
          </li>
        ))}
        {items.length === 0 && <li className="muted">No cases yet. They appear after you use GitHub Debug.</li>}
      </ul>
      {items.length > 0 && (
        <section className="row end">
          <ConfirmButton label="Delete issue history" onConfirm={() => api.clearIssues().then(load).then(() => flash("Issue history deleted"))} />
        </section>
      )}
    </>
  );
}
