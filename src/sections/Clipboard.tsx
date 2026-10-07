import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "../api";
import type { ClipItem } from "../api";
import { ConfirmButton, NumberField, Toggle } from "../components/common";
import type { SectionProps } from "./types";
import { ALT, CMD } from "../platform";

const KIND_LABEL = { student: "Student", reply: "Reply", copied: "Copied" };

export default function Clipboard({ settings, update, flash }: SectionProps) {
  const [items, setItems] = useState<ClipItem[]>([]);
  const [kind, setKind] = useState<"all" | ClipItem["kind"]>("all");
  const [filter, setFilter] = useState("");
  const [picked, setPicked] = useState<number[]>([]);
  const [caseCount, setCaseCount] = useState(0);

  const load = useCallback(async () => {
    try {
      setItems(await api.listClipboard());
      setCaseCount(await api.caseStatus());
    } catch (e) {
      flash(String(e), true);
    }
  }, [flash]);

  useEffect(() => {
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return items.filter((i) => (kind === "all" || i.kind === kind) && (!q || i.content.toLowerCase().includes(q)));
  }, [items, kind, filter]);

  const toggle = (id: number) => setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));

  const addToCase = async () => {
    const n = await api.caseAddItems(picked);
    setPicked([]);
    setCaseCount(n);
    flash(`Case has ${n} message${n === 1 ? "" : "s"}. Press ${ALT}R or ${ALT}G in your support chat.`);
  };

  return (
    <>
      <section className="toggles">
        <Toggle checked={settings.clipboardHistory} onChange={(v) => update("clipboardHistory", v)}>
          Keep a history of student messages and replies
        </Toggle>
        <Toggle checked={settings.clipboardWatch} onChange={(v) => update("clipboardWatch", v)}>
          Also record text I copy ({CMD}C) <span className="muted small">· password-manager copies are never recorded</span>
        </Toggle>
      </section>
      <section className="grid">
        <NumberField label="Keep at most (items)" value={settings.clipboardMaxItems} min={10} max={2000} onChange={(v) => update("clipboardMaxItems", v)} />
        <NumberField label="Forget after (days)" value={settings.clipboardMaxDays} min={1} max={365} onChange={(v) => update("clipboardMaxDays", v)} />
      </section>
      <p className="muted small">Stored only on this Mac. Pick several messages to answer them as one case.</p>

      <section className="row">
        <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)} style={{ width: "auto" }}>
          <option value="all">All</option>
          <option value="student">Student messages</option>
          <option value="reply">Replies</option>
          <option value="copied">Copied</option>
        </select>
        <input className="grow" placeholder={`Search ${items.length} items`} value={filter} onChange={(e) => setFilter(e.target.value)} />
      </section>

      {(picked.length > 0 || caseCount > 0) && (
        <div className="banner">
          <div className="row spread">
            <span>
              {picked.length > 0 ? `${picked.length} selected. ` : ""}
              {caseCount > 0 ? `Case has ${caseCount} message${caseCount === 1 ? "" : "s"}.` : ""}
            </span>
            <div className="row">
              {picked.length > 0 && <button className="primary" onClick={addToCase}>Add to case</button>}
              {caseCount > 0 && <button onClick={() => api.caseClear().then(load)}>Clear case</button>}
            </div>
          </div>
        </div>
      )}

      <ul className="list">
        {shown.map((i) => (
          <li key={i.id}>
            <input type="checkbox" checked={picked.includes(i.id)} onChange={() => toggle(i.id)} aria-label="Select" />
            <div className="grow clip">
              <div className="clip">{i.content}</div>
              <div className="muted small">{KIND_LABEL[i.kind]} · {api.timeAgo(i.createdAt)}{i.source ? ` · ${i.source}` : ""}</div>
            </div>
            <button className="link" onClick={() => api.copyClipboardItem(i.id).then(() => flash("Copied"))}>Copy</button>
            <button className="link danger-text" onClick={() => api.deleteClipboardItem(i.id).then(load)}>Delete</button>
          </li>
        ))}
        {items.length === 0 && <li className="muted">Nothing yet.</li>}
      </ul>
      {items.length > 0 && (
        <section className="row end">
          <ConfirmButton label="Clear clipboard history" onConfirm={() => api.clearClipboard().then(load).then(() => flash("Clipboard history cleared"))} />
        </section>
      )}
    </>
  );
}
