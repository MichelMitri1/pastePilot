import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "../api";
import type { Analysis, DebugContext, Finding } from "../api";
import Diff from "./Diff";
import { imagesFrom, toDataUrl } from "./screenshots";
import "./debug.css";

type Phase = "idle" | "analyzing" | "writing" | "ready";

const CONFIDENCE_LABEL = { high: "High confidence", medium: "Medium confidence", low: "Low confidence" };
const MAX_SCREENSHOTS = 3;

export default function DebugApp() {
  const [ctx, setCtx] = useState<DebugContext | null>(null);
  const [repoUrl, setRepoUrl] = useState("");
  const [issue, setIssue] = useState("");
  const [files, setFiles] = useState<string[]>([""]);
  const [shots, setShots] = useState<string[]>([]);
  const [compare, setCompare] = useState(false);
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [reply, setReply] = useState("");
  const [includeSnippet, setIncludeSnippet] = useState(false);
  const [copied, setCopied] = useState(false);
  const [dragging, setDragging] = useState(false);
  const streaming = useRef(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const applyContext = useCallback((c: DebugContext) => {
    setCtx(c);
    if (c.issue) setIssue(c.issue);
    if (c.repoUrl) setRepoUrl((current) => current || c.repoUrl);
    setCompare(c.compareCommitsDefault);
    setAnalysis(null);
    setReply("");
    setError(null);
    setPhase("idle");
  }, []);

  useEffect(() => {
    api.debugGetContext().then(applyContext);
    const subs = [
      listen<DebugContext>("debug-context", (e) => applyContext(e.payload)),
      listen<string>("debug-progress", (e) => setProgress(e.payload)),
      listen<string>("debug-reply-delta", (e) => {
        if (streaming.current) setReply((r) => r + e.payload);
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, [applyContext]);

  const addImages = useCallback(async (images: File[]) => {
    if (!images.length) return;
    try {
      const urls = await Promise.all(images.map((f) => toDataUrl(f)));
      setShots((s) => [...s, ...urls].slice(0, MAX_SCREENSHOTS));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // ⌘V a screenshot anywhere in the window.
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

  const busy = phase === "analyzing" || phase === "writing";

  const writeReply = useCallback(async (snippet: boolean) => {
    setPhase("writing");
    setReply("");
    streaming.current = true;
    try {
      const final = await api.debugGenerateReply(snippet);
      setReply(final);
      setPhase("ready");
    } catch (e) {
      setError(String(e));
      setPhase("idle");
    } finally {
      streaming.current = false;
      setProgress("");
    }
  }, []);

  const analyze = useCallback(async () => {
    if (busy) return;
    setError(null);
    setAnalysis(null);
    setReply("");
    if (!repoUrl.trim() && !shots.length) return setError("Paste the student's GitHub repository URL, or add a screenshot.");
    if (!issue.trim() && !shots.length) return setError("Add the student's message or a screenshot.");
    setPhase("analyzing");
    try {
      const result = await api.debugAnalyze({
        repoUrl: repoUrl.trim(),
        issue: issue.trim(),
        files: files.map((f) => f.trim()).filter(Boolean),
        screenshots: shots,
        compareCommits: compare,
      });
      setAnalysis(result);
      const canSnippet = result.status === "found" && result.findings.some((f) => f.after);
      const snippet = canSnippet && includeSnippet;
      await writeReply(snippet);
    } catch (e) {
      setError(String(e));
      setPhase("idle");
      setProgress("");
    }
  }, [busy, repoUrl, issue, files, shots, compare, includeSnippet, writeReply]);

  const paste = useCallback(async () => {
    if (!reply.trim() || busy) return;
    try {
      await api.debugPaste(reply);
    } catch (e) {
      setError(String(e));
    }
  }, [reply, busy]);

  const copy = async (text: string) => {
    await api.debugCopy(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && e.metaKey && e.shiftKey) {
        e.preventDefault();
        paste();
      } else if (e.key === "Enter" && e.metaKey) {
        e.preventDefault();
        analyze();
      } else if (e.key === "Escape") {
        getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [analyze, paste]);

  const canSnippet = !!analysis && analysis.status === "found" && analysis.findings.some((f) => f.after);

  return (
    <div
      className={dragging ? "debug dragging" : "debug"}
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
        <h2>GitHub Debug</h2>
        <span className="muted small">
          {ctx?.hasTarget
            ? ctx.conversation
              ? `${ctx.conversation}${ctx.historyCount ? ` · ${ctx.historyCount} earlier messages used as context` : ""}`
              : "Reply will be pasted into your support chat"
            : "No support chat detected: the reply will be copied instead of pasted"}
        </span>
      </header>

      <section>
        <label>GitHub repository <span className="muted">(detected from the message when possible)</span></label>
        <input placeholder="https://github.com/user/project" value={repoUrl} onChange={(e) => setRepoUrl(e.target.value)} />
      </section>

      <section>
        <div className="row spread">
          <label>Student issue</label>
          {!!ctx?.caseMessages && <span className="tag">Combined from {ctx.caseMessages} messages</span>}
        </div>
        <textarea rows={4} placeholder="Select the student's message before pressing ⌥G, or paste it here." value={issue} onChange={(e) => setIssue(e.target.value)} />
      </section>

      <section>
        <label>Screenshots <span className="muted">(optional: paste with ⌘V, drop, or choose)</span></label>
        <div className="shots">
          {shots.map((s, i) => (
            <div className="shot" key={i}>
              <img src={s} alt={`Screenshot ${i + 1}`} />
              <button className="remove" aria-label="Remove screenshot" onClick={() => setShots(shots.filter((_, j) => j !== i))}>✕</button>
            </div>
          ))}
          {shots.length < MAX_SCREENSHOTS && (
            <button className="shot add" onClick={() => fileInput.current?.click()}>+ Add</button>
          )}
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            multiple
            hidden
            onChange={(e) => {
              addImages(imagesFrom(e.target.files));
              e.target.value = "";
            }}
          />
        </div>
      </section>

      <details className="more">
        <summary className="muted small">Relevant files (optional, detected automatically) · commit comparison</summary>
        <div className="files">
          {files.map((f, i) => (
            <div className="row" key={i}>
              <input
                placeholder={i === 0 ? "Navbar.jsx, src/styles.css…" : "Another file"}
                value={f}
                onChange={(e) => setFiles(files.map((x, j) => (j === i ? e.target.value : x)))}
              />
              {files.length > 1 && <button className="link" onClick={() => setFiles(files.filter((_, j) => j !== i))}>Remove</button>}
            </div>
          ))}
          <button className="link add" onClick={() => setFiles([...files, ""])}>+ Add file</button>
          <label className="check">
            <input type="checkbox" checked={compare} onChange={(e) => setCompare(e.target.checked)} />
            <span>Compare the 3 most recent commits <span className="muted small">(on automatically when the student says it broke after a change)</span></span>
          </label>
        </div>
      </details>

      <div className="row">
        <button className="primary" disabled={busy} onClick={analyze}>
          {phase === "analyzing" ? "Analyzing…" : repoUrl.trim() ? "Analyze Repository" : "Analyze"}
        </button>
        <span className="muted small">{busy ? progress : "⌘↩ to analyze · read-only, nothing is run or changed"}</span>
      </div>

      {error && <div className="banner error-banner">{error}</div>}

      {analysis && <Results analysis={analysis} onCopy={copy} />}

      {(phase === "writing" || reply) && (
        <section className="card">
          <div className="row spread">
            <label>{analysis?.status === "uncertain" ? "Reply (asking for what's missing)" : "Reply"}</label>
            {canSnippet && (
              <label className="check">
                <input
                  type="checkbox"
                  checked={includeSnippet}
                  disabled={busy}
                  onChange={(e) => {
                    setIncludeSnippet(e.target.checked);
                    writeReply(e.target.checked);
                  }}
                />
                <span>Include code snippet</span>
              </label>
            )}
          </div>
          <textarea rows={7} value={reply} readOnly={phase === "writing"} onChange={(e) => setReply(e.target.value)} />
          <div className="row spread">
            <div className="row">
              <button className="primary" disabled={busy || !reply.trim()} onClick={paste}>Paste Reply</button>
              <button disabled={busy || !reply.trim()} onClick={() => copy(reply)}>{copied ? "Copied" : "Copy"}</button>
              <button disabled={busy || !analysis} onClick={() => writeReply(canSnippet && includeSnippet)}>Rewrite</button>
            </div>
            <span className="muted small">⌘⇧↩ to paste · never sends</span>
          </div>
        </section>
      )}
    </div>
  );
}

function Results({ analysis, onCopy }: { analysis: Analysis; onCopy: (t: string) => void }) {
  const found = analysis.status === "found";
  return (
    <section className="card results">
      <div className="row spread wrap-row">
        <strong>{found ? "Issue found" : "Not enough evidence yet"}</strong>
        <div className="row wrap-row">
          {analysis.project && <span className="tag">{analysis.project.label}</span>}
          <span className="tag">{analysis.mode}</span>
          <span className={`confidence ${analysis.confidence}`}>{CONFIDENCE_LABEL[analysis.confidence]}</span>
        </div>
      </div>
      {analysis.summary && <p>{analysis.summary}</p>}
      {!found && (
        <div className="ask">
          <strong>Asking instead of guessing.</strong> The reply will ask the student for: {analysis.missingInfo}
        </div>
      )}

      {analysis.findings.map((f, i) => (
        <FindingView key={i} finding={f} tentative={!found} repoKnown={!!analysis.repo} projectLabel={analysis.project?.label ?? ""} issueType={analysis.issueType} onCopy={onCopy} />
      ))}

      {analysis.checks.length > 0 && (
        <details open={found ? undefined : true}>
          <summary className="small"><strong>Automated checks ({analysis.checks.length})</strong></summary>
          <ul className="checks">
            {analysis.checks.map((c, i) => (
              <li key={i}>
                <span className="tag">{c.kind}</span> <span className="mono">{c.file}{c.line ? `:${c.line}` : ""}</span> {c.message}
              </li>
            ))}
          </ul>
        </details>
      )}

      {analysis.commits.length > 0 && (
        <details>
          <summary className="small"><strong>Recent commits ({analysis.commits.length})</strong></summary>
          <ul className="checks">
            {analysis.commits.map((c) => (
              <li key={c.sha}>
                <button className="link mono" onClick={() => c.url && api.openGithubUrl(c.url)}>{c.short}</button>{" "}
                {c.message} <span className="muted small">· {c.date.slice(0, 10)} · {c.files.map((f) => f.path.split("/").pop()).join(", ")}</span>
              </li>
            ))}
          </ul>
        </details>
      )}

      {analysis.similar.length > 0 && (
        <details>
          <summary className="small"><strong>Seen before ({analysis.similar.length})</strong></summary>
          <ul className="checks">
            {analysis.similar.map((s) => (
              <li key={s.id}>
                <span className="muted small">{api.timeAgo(s.createdAt)} · {s.sameRepo ? "same repo" : s.repo || "screenshot"}:</span> “{s.issue}” → {s.summary || "no diagnosis"}
              </li>
            ))}
          </ul>
        </details>
      )}

      {analysis.fixes.length > 0 && (
        <div className="small">
          <span className="muted">Saved fixes considered: </span>
          {analysis.fixes.map((f) => (
            <span key={f.id} className={f.used ? "tag used" : "tag"}>{f.used ? "✓ " : ""}{f.title}</span>
          ))}
        </div>
      )}

      {(analysis.examined.length > 0 || analysis.screenshots > 0) && (
        <details>
          <summary className="muted small">
            {analysis.examined.length > 0 && `Read ${analysis.examined.length} file${analysis.examined.length === 1 ? "" : "s"} from ${analysis.repo}`}
            {analysis.examined.length > 0 && analysis.screenshots > 0 && " · "}
            {analysis.screenshots > 0 && `${analysis.screenshots} screenshot${analysis.screenshots === 1 ? "" : "s"}`}
          </summary>
          <ul className="examined">
            {analysis.examined.map((e) => (
              <li key={e.path}>
                <button className="link" onClick={() => api.openGithubUrl(e.url)}>{e.path}</button>
                <span className="muted small"> {e.lines} lines{e.truncated ? " (partly)" : ""}</span>
              </li>
            ))}
          </ul>
          {analysis.notes.map((n, i) => <div key={i} className="muted small">{n}</div>)}
        </details>
      )}
    </section>
  );
}

function FindingView({ finding: f, tentative, repoKnown, projectLabel, issueType, onCopy }: {
  finding: Finding; tentative: boolean; repoKnown: boolean; projectLabel: string; issueType: string; onCopy: (t: string) => void;
}) {
  const [view, setView] = useState<"diff" | "code">(f.before && f.after ? "diff" : "code");
  const [saved, setSaved] = useState(false);
  const location = f.lineStart ? `${f.file}:${f.lineStart}${f.lineEnd && f.lineEnd !== f.lineStart ? `-${f.lineEnd}` : ""}` : f.file;

  const saveFix = async () => {
    await api.saveFix({
      title: f.cause.length > 80 ? f.cause.slice(0, 77) + "…" : f.cause,
      problem: f.cause,
      solution: f.fix || "See snippet",
      snippet: f.after ?? "",
      tags: issueType,
      projectType: projectLabel,
    });
    setSaved(true);
  };

  return (
    <div className="finding">
      <div className="row spread wrap-row">
        {tentative && <span className="tag">Possible lead (unconfirmed)</span>}
        {f.url ? (
          <button className="link mono" onClick={() => api.openGithubUrl(f.url!)}>{location}</button>
        ) : (
          location && <span className="mono">{location}</span>
        )}
        <div className="row">
          {f.before && f.after && (
            <div className="segmented">
              <button className={view === "diff" ? "on" : ""} onClick={() => setView("diff")}>Before/After</button>
              <button className={view === "code" ? "on" : ""} onClick={() => setView("code")}>Code</button>
            </div>
          )}
          {f.after && <button onClick={() => onCopy(f.after!)}>Copy fix</button>}
          {!tentative && <button disabled={saved} onClick={saveFix}>{saved ? "Saved" : "Save as fix"}</button>}
        </div>
      </div>
      {view === "diff" && f.before && f.after ? (
        <Diff before={f.before} after={f.after} startLine={f.lineStart} />
      ) : (
        f.snippet && (
          <pre className="snippet">
            {f.snippet.lines.map((line, i) => {
              const n = f.snippet!.startLine + i;
              const hot = n >= f.snippet!.highlightStart && n <= f.snippet!.highlightEnd;
              return (
                <div key={n} className={hot ? "hot" : ""}>
                  <span className="ln">{n}</span>
                  {line || " "}
                </div>
              );
            })}
          </pre>
        )
      )}
      {!repoKnown && f.after && !f.before && <pre className="snippet"><div>{f.after}</div></pre>}
      <div><span className="muted">Likely cause:</span> {f.cause}</div>
      {f.fix && <div><span className="muted">Suggested fix:</span> {f.fix}</div>}
    </div>
  );
}
