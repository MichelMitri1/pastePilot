import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "../api";
import type { Preset, ReviewContext, ReviewItem, ReviewResult } from "../api";
import Diff from "../debug/Diff";
import { imagesFrom, toDataUrl } from "../debug/screenshots";
import "../debug/debug.css";
import "./review.css";

type Phase = "idle" | "reviewing" | "writing" | "ready";
const VIEWPORTS = ["desktop", "tablet", "mobile"] as const;
const MAX_SHOTS = 4;

const STATUS_ICON: Record<string, string> = { complete: "✅", needs_fix: "⚠️", missing: "❌", broken: "⚠️", unable_to_verify: "❔" };
const STATUS_LABEL: Record<string, string> = {
  complete: "Complete", needs_fix: "Needs Fix", missing: "Missing", broken: "Broken", unable_to_verify: "Unable to Verify",
};
const SEVERITY_GROUPS = [
  { key: "critical", label: "Critical" },
  { key: "needs_fix", label: "Needs Fix" },
  { key: "minor", label: "Minor" },
] as const;

export default function ReviewApp() {
  const [ctx, setCtx] = useState<ReviewContext | null>(null);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [presetId, setPresetId] = useState<number | "">("");
  const [exampleUrl, setExampleUrl] = useState("");
  const [studentUrl, setStudentUrl] = useState("");
  const [repoUrl, setRepoUrl] = useState("");
  const [requirements, setRequirements] = useState("");
  const [notes, setNotes] = useState("");
  const [shots, setShots] = useState<string[]>([]);
  const [viewports, setViewports] = useState<string[]>([...VIEWPORTS]);
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ReviewResult | null>(null);
  const [reply, setReply] = useState("");
  const [length, setLength] = useState("normal");
  const [includeMinor, setIncludeMinor] = useState(false);
  const [copied, setCopied] = useState(false);
  const [presetName, setPresetName] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const streaming = useRef(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const applyContext = useCallback((c: ReviewContext) => {
    setCtx(c);
    setPresets(c.presets);
    if (c.studentUrl) setStudentUrl(c.studentUrl);
    if (c.repoUrl) setRepoUrl(c.repoUrl);
    if (c.screenshot) setShots((s) => (s.includes(c.screenshot!) ? s : [c.screenshot!, ...s].slice(0, MAX_SHOTS)));
    setResult(null);
    setReply("");
    setError(null);
    setPhase("idle");
  }, []);

  useEffect(() => {
    api.reviewGetContext().then(applyContext);
    const subs = [
      listen<ReviewContext>("review-context", (e) => applyContext(e.payload)),
      listen<string>("review-progress", (e) => setProgress(e.payload)),
      listen<string>("review-reply-delta", (e) => {
        if (streaming.current) setReply((r) => r + e.payload);
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, [applyContext]);

  const addImages = useCallback(async (images: File[]) => {
    if (!images.length) return;
    try {
      const urls = await Promise.all(images.map((f) => toDataUrl(f)));
      setShots((s) => [...s, ...urls].slice(0, MAX_SHOTS));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const images = imagesFrom(e.clipboardData?.items ?? null);
      if (images.length) {
        e.preventDefault();
        addImages(images);
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [addImages]);

  const busy = phase === "reviewing" || phase === "writing";

  const writeFeedback = useCallback(async (len: string, minor: boolean) => {
    setPhase("writing");
    setReply("");
    streaming.current = true;
    try {
      setReply(await api.reviewFeedback({ length: len, includeMinor: minor }));
      setPhase("ready");
    } catch (e) {
      setError(String(e));
      setPhase("ready");
    } finally {
      streaming.current = false;
      setProgress("");
    }
  }, []);

  const review = useCallback(async () => {
    if (busy) return;
    setError(null);
    if (!exampleUrl.trim() || !studentUrl.trim()) return setError("Add the example website and the student's live website.");
    setResult(null);
    setReply("");
    setPhase("reviewing");
    try {
      const r = await api.reviewRun({ exampleUrl, studentUrl, repoUrl, requirements, notes, screenshots: shots, viewports });
      setResult(r);
      await writeFeedback(length, includeMinor);
    } catch (e) {
      setError(String(e));
      setPhase("idle");
      setProgress("");
    }
  }, [busy, exampleUrl, studentUrl, repoUrl, requirements, notes, shots, viewports, length, includeMinor, writeFeedback]);

  const paste = useCallback(async () => {
    if (!reply.trim() || busy) return;
    try {
      await api.reviewPaste(reply);
    } catch (e) {
      setError(String(e));
    }
  }, [reply, busy]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && e.metaKey && e.shiftKey) {
        e.preventDefault();
        paste();
      } else if (e.key === "Enter" && e.metaKey) {
        e.preventDefault();
        review();
      } else if (e.key === "Escape") {
        getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [review, paste]);

  const choosePreset = (id: string) => {
    if (!id) return setPresetId("");
    const p = presets.find((x) => String(x.id) === id);
    if (p) {
      setPresetId(p.id!);
      setExampleUrl(p.exampleUrl);
      setRequirements(p.requirements);
    }
  };

  const savePreset = async () => {
    if (!presetName?.trim()) return;
    try {
      const list = await api.reviewSavePreset({ id: presetId || null, name: presetName.trim(), exampleUrl, requirements });
      setPresets(list);
      setPresetName(null);
      const saved = list.find((p) => p.name === presetName.trim());
      if (saved?.id) setPresetId(saved.id);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div
      className={dragging ? "debug review dragging" : "debug review"}
      onDragOver={(e) => {
        e.preventDefault();
        setDragging(true);
      }}
      onDragLeave={() => setDragging(false)}
      onDrop={(e) => {
        e.preventDefault();
        setDragging(false);
        addImages(imagesFrom(e.dataTransfer.files));
      }}
    >
      <header>
        <h2>Assignment Review</h2>
        <span className="muted small">
          {ctx?.hasTarget
            ? ctx.conversation ? `${ctx.conversation} · feedback will be pasted into this chat` : "Feedback will be pasted into your support chat"
            : "No support chat detected: feedback will be copied instead of pasted"}
        </span>
        {ctx && !ctx.chrome && (
          <div className="banner">Google Chrome isn't installed, so sites are read without JavaScript or screenshots. Install Chrome for full visual reviews.</div>
        )}
      </header>

      <section className="row wrap-row preset-row">
        <label>Assignment</label>
        <select value={presetId === "" ? "" : String(presetId)} onChange={(e) => choosePreset(e.target.value)} style={{ width: "auto", flex: 1 }}>
          <option value="">New / unsaved</option>
          {presets.map((p) => <option key={p.id ?? p.name} value={String(p.id)}>{p.name}</option>)}
        </select>
        {presetName === null ? (
          <>
            <button disabled={!exampleUrl.trim()} onClick={() => setPresetName(presets.find((p) => p.id === presetId)?.name ?? "")}>Save preset</button>
            {presetId !== "" && <button className="link danger-text" onClick={() => api.reviewDeletePreset(presetId as number).then((l) => { setPresets(l); setPresetId(""); })}>Delete</button>}
          </>
        ) : (
          <>
            <input autoFocus placeholder="Name, e.g. Week 3 Landing Page" value={presetName} onChange={(e) => setPresetName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && savePreset()} style={{ flex: 1 }} />
            <button className="primary" onClick={savePreset}>Save</button>
            <button onClick={() => setPresetName(null)}>Cancel</button>
          </>
        )}
      </section>

      <div className="grid three-up">
        <section>
          <label>Example website</label>
          <input placeholder="https://example-project.netlify.app" value={exampleUrl} onChange={(e) => setExampleUrl(e.target.value)} />
        </section>
        <section>
          <label>Student live website</label>
          <input placeholder="https://student.github.io/project" value={studentUrl} onChange={(e) => setStudentUrl(e.target.value)} />
        </section>
        <section>
          <label>Student GitHub repository</label>
          <input placeholder="https://github.com/student/project" value={repoUrl} onChange={(e) => setRepoUrl(e.target.value)} />
        </section>
      </div>

      <details className="more" open={!!requirements || !!notes}>
        <summary className="muted small">Requirements, notes and screenshots (optional)</summary>
        <div className="files">
          <label>Assignment requirements <span className="muted">(one per line; reviewed first when given)</span></label>
          <textarea rows={4} placeholder={"- Responsive navbar with a mobile menu\n- Products section with 4 cards\n- Footer with social links"} value={requirements} onChange={(e) => setRequirements(e.target.value)} />
          <label>Notes for the review</label>
          <textarea rows={2} placeholder="e.g. focus on the mobile layout" value={notes} onChange={(e) => setNotes(e.target.value)} />
          <label>Screenshots <span className="muted">(paste with ⌘V, drop, or choose)</span></label>
          <div className="shots">
            {shots.map((s, i) => (
              <div className="shot" key={i}>
                <img src={s} alt={`Screenshot ${i + 1}`} />
                <button className="remove" aria-label="Remove screenshot" onClick={() => setShots(shots.filter((_, j) => j !== i))}>✕</button>
              </div>
            ))}
            {shots.length < MAX_SHOTS && <button className="shot add" onClick={() => fileInput.current?.click()}>+ Add</button>}
            <input ref={fileInput} type="file" accept="image/*" multiple hidden onChange={(e) => { addImages(imagesFrom(e.target.files)); e.target.value = ""; }} />
          </div>
        </div>
      </details>

      <div className="row wrap-row">
        <button className="primary" disabled={busy} onClick={review}>{phase === "reviewing" ? "Reviewing…" : "Review Assignment"}</button>
        <span className="muted small">Viewports:</span>
        {VIEWPORTS.map((v) => (
          <label key={v} className="check">
            <input type="checkbox" checked={viewports.includes(v)} onChange={(e) => setViewports(e.target.checked ? [...viewports, v] : viewports.filter((x) => x !== v))} />
            <span>{v}</span>
          </label>
        ))}
        <span className="muted small">{busy ? progress : "⌘↩ · read-only: nothing is run, changed or sent"}</span>
      </div>

      {error && <div className="banner error-banner">{error}</div>}

      {result && <Results result={result} />}

      {(phase === "writing" || reply) && (
        <section className="card">
          <div className="row spread wrap-row">
            <label>Feedback</label>
            <div className="row wrap-row">
              <div className="segmented">
                {["short", "normal", "detailed"].map((l) => (
                  <button key={l} className={length === l ? "on" : ""} disabled={busy} onClick={() => { setLength(l); writeFeedback(l, includeMinor); }}>
                    {l === "short" ? "Shorter" : l === "detailed" ? "More detailed" : "Normal"}
                  </button>
                ))}
              </div>
              <label className="check">
                <input type="checkbox" checked={includeMinor} disabled={busy} onChange={(e) => { setIncludeMinor(e.target.checked); writeFeedback(length, e.target.checked); }} />
                <span>Include minor issues</span>
              </label>
            </div>
          </div>
          <textarea rows={7} value={reply} readOnly={phase === "writing"} onChange={(e) => setReply(e.target.value)} />
          <div className="row spread">
            <div className="row">
              <button className="primary" disabled={busy || !reply.trim()} onClick={paste}>Paste Reply</button>
              <button disabled={busy || !reply.trim()} onClick={async () => { await api.reviewCopy(reply); setCopied(true); setTimeout(() => setCopied(false), 1500); }}>{copied ? "Copied" : "Copy"}</button>
              <button disabled={busy || !result} onClick={() => writeFeedback(length, includeMinor)}>Regenerate</button>
            </div>
            <span className="muted small">⌘⇧↩ to paste · never sends</span>
          </div>
        </section>
      )}
    </div>
  );
}

function Results({ result }: { result: ReviewResult }) {
  const count = (sev: string) => result.items.filter((i) => i.severity === sev).length;
  const passed = result.items.filter((i) => i.status === "complete");
  const unverified = result.items.filter((i) => i.status === "unable_to_verify");
  const [vp, setVp] = useState(result.student.shots[0]?.viewport ?? result.example.shots[0]?.viewport ?? "desktop");
  const viewportsWithShots = [...new Set([...result.example.shots, ...result.student.shots].map((s) => s.viewport))];

  return (
    <section className="card results">
      <div className="row spread wrap-row">
        <strong>Assignment Review</strong>
        <div className="row wrap-row">
          {result.project && <span className="tag">{result.project.label}</span>}
          <span className="pill critical">{count("critical")} critical</span>
          <span className="pill needs">{count("needs_fix")} needs fix</span>
          <span className="pill minor">{count("minor")} minor</span>
          <span className="pill passed">{passed.length} passed</span>
        </div>
      </div>
      {result.summary && <p>{result.summary}</p>}
      {result.strengths.length > 0 && <p className="small"><span className="muted">Done well:</span> {result.strengths.join(" · ")}</p>}

      {result.requirements.length > 0 && (
        <div className="reqs">
          <label>Requirements</label>
          {result.requirements.map((q, i) => (
            <div key={i} className="req">
              <span aria-hidden>{STATUS_ICON[q.status]}</span>
              <span><strong>{q.requirement}</strong> <span className="muted small">· {STATUS_LABEL[q.status]}</span>{q.note && <span className="muted small"> · {q.note}</span>}</span>
            </div>
          ))}
        </div>
      )}

      {SEVERITY_GROUPS.map((g) => {
        const items = result.items.filter((i) => i.severity === g.key);
        return items.length ? (
          <div key={g.key} className={`group ${g.key}`}>
            <h4>{g.label}</h4>
            {items.map((item, i) => <Item key={i} item={item} />)}
          </div>
        ) : null;
      })}

      {passed.length > 0 && (
        <details>
          <summary className="small"><strong>Passed ({passed.length})</strong></summary>
          <ul className="checks">{passed.map((p, i) => <li key={i}>✅ {p.area}{p.detail && <span className="muted small"> · {p.detail}</span>}</li>)}</ul>
        </details>
      )}
      {unverified.length > 0 && (
        <details>
          <summary className="small"><strong>Unable to verify ({unverified.length})</strong></summary>
          <ul className="checks">{unverified.map((p, i) => <li key={i}>❔ {p.area}{p.detail && <span className="muted small"> · {p.detail}</span>}</li>)}</ul>
        </details>
      )}

      {viewportsWithShots.length > 0 && (
        <details>
          <summary className="small"><strong>Screenshots: example vs student</strong></summary>
          <div className="segmented" style={{ margin: "8px 0" }}>
            {viewportsWithShots.map((v) => <button key={v} className={vp === v ? "on" : ""} onClick={() => setVp(v)}>{v}</button>)}
          </div>
          <div className="compare">
            {[["Example", result.example], ["Student", result.student]].map(([label, site]) => {
              const s = (site as typeof result.example).shots.find((x) => x.viewport === vp);
              return (
                <figure key={label as string}>
                  <figcaption className="muted small">{label as string}</figcaption>
                  {s ? <img src={s.dataUrl} alt={`${label} at ${vp}`} /> : <div className="muted small">No screenshot</div>}
                </figure>
              );
            })}
          </div>
        </details>
      )}

      {(result.student.consoleErrors.length > 0 || result.student.brokenLinks.length > 0) && (
        <details>
          <summary className="small"><strong>Student site problems detected automatically</strong></summary>
          <ul className="checks">
            {result.student.consoleErrors.map((c, i) => <li key={`c${i}`}><span className="tag">console</span> <span className="mono">{c}</span></li>)}
            {result.student.brokenLinks.map((b, i) => <li key={`b${i}`}><span className="tag">link</span> {b}</li>)}
          </ul>
        </details>
      )}

      {result.checks.length > 0 && (
        <details>
          <summary className="small"><strong>Code checks ({result.checks.length})</strong></summary>
          <ul className="checks">{result.checks.map((c, i) => <li key={i}><span className="tag">{c.kind}</span> <span className="mono">{c.file}{c.line ? `:${c.line}` : ""}</span> {c.message}</li>)}</ul>
        </details>
      )}

      {(result.examined.length > 0 || result.notes.length > 0) && (
        <details>
          <summary className="muted small">{result.examined.length ? `Read ${result.examined.length} file${result.examined.length === 1 ? "" : "s"} from ${result.repo}` : "Notes"}</summary>
          <ul className="examined">{result.examined.map((f) => <li key={f} className="mono">{f}</li>)}</ul>
          {result.notes.map((n, i) => <div key={i} className="muted small">{n}</div>)}
        </details>
      )}
    </section>
  );
}

function Item({ item }: { item: ReviewItem }) {
  const [view, setView] = useState<"diff" | "code">(item.before && item.after ? "diff" : "code");
  const location = item.file ? `${item.file}${item.lineStart ? `:${item.lineStart}${item.lineEnd && item.lineEnd !== item.lineStart ? `-${item.lineEnd}` : ""}` : ""}` : null;
  return (
    <div className="finding">
      <div className="row spread wrap-row">
        <span><span aria-hidden>{STATUS_ICON[item.status]}</span> <strong>{item.area}</strong> <span className="muted small">· {STATUS_LABEL[item.status]}</span></span>
        {item.viewport !== "all" && <span className="tag">{item.viewport}</span>}
      </div>
      {item.detail && <div>{item.detail}</div>}
      {location && (
        <div className="row wrap-row">
          <span className="muted small">Likely file:</span>
          {item.url ? <button className="link mono" onClick={() => api.openGithubUrl(item.url!)}>{location}</button> : <span className="mono">{location}</span>}
          {item.before && item.after && (
            <div className="segmented">
              <button className={view === "diff" ? "on" : ""} onClick={() => setView("diff")}>Before/After</button>
              <button className={view === "code" ? "on" : ""} onClick={() => setView("code")}>Code</button>
            </div>
          )}
        </div>
      )}
      {view === "diff" && item.before && item.after ? (
        <Diff before={item.before} after={item.after} startLine={item.lineStart} />
      ) : (
        item.snippet && (
          <pre className="snippet">
            {item.snippet.lines.map((line, i) => {
              const n = item.snippet!.startLine + i;
              const hot = n >= item.snippet!.highlightStart && n <= item.snippet!.highlightEnd;
              return <div key={n} className={hot ? "hot" : ""}><span className="ln">{n}</span>{line || " "}</div>;
            })}
          </pre>
        )
      )}
      {item.cause && <div><span className="muted">Cause:</span> {item.cause}</div>}
      {item.fix && <div><span className="muted">Suggested fix:</span> {item.fix}</div>}
    </div>
  );
}
