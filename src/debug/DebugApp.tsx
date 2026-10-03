import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "../api";
import type { Analysis, DebugContext, Finding } from "../api";
import "./debug.css";

type Phase = "idle" | "analyzing" | "writing" | "ready";

const CONFIDENCE_LABEL = { high: "High confidence", medium: "Medium confidence", low: "Low confidence" };

export default function DebugApp() {
  const [ctx, setCtx] = useState<DebugContext | null>(null);
  const [repoUrl, setRepoUrl] = useState("");
  const [issue, setIssue] = useState("");
  const [files, setFiles] = useState<string[]>([""]);
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [reply, setReply] = useState("");
  const [copied, setCopied] = useState(false);
  const streaming = useRef(false);

  const applyContext = useCallback((c: DebugContext) => {
    setCtx(c);
    if (c.issue) setIssue(c.issue);
    if (c.repoUrl) setRepoUrl((current) => current || c.repoUrl);
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

  const busy = phase === "analyzing" || phase === "writing";

  const writeReply = useCallback(async () => {
    setPhase("writing");
    setReply("");
    streaming.current = true;
    try {
      const final = await api.debugGenerateReply();
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
    if (!repoUrl.trim()) return setError("Paste the student's GitHub repository URL.");
    if (!issue.trim()) return setError("Add the student's message.");
    setPhase("analyzing");
    try {
      const result = await api.debugAnalyze({
        repoUrl: repoUrl.trim(),
        issue: issue.trim(),
        files: files.map((f) => f.trim()).filter(Boolean),
      });
      setAnalysis(result);
      await writeReply();
    } catch (e) {
      setError(String(e));
      setPhase("idle");
      setProgress("");
    }
  }, [busy, repoUrl, issue, files, writeReply]);

  const paste = useCallback(async () => {
    if (!reply.trim() || busy) return;
    try {
      await api.debugPaste(reply);
    } catch (e) {
      setError(String(e));
    }
  }, [reply, busy]);

  const copy = async () => {
    await api.debugCopy(reply);
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

  return (
    <div className="debug">
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
        <label>GitHub repository</label>
        <input placeholder="https://github.com/user/project" value={repoUrl} onChange={(e) => setRepoUrl(e.target.value)} />
      </section>

      <section>
        <label>Student issue</label>
        <textarea rows={4} placeholder="Select the student's message before pressing ⌥G, or paste it here." value={issue} onChange={(e) => setIssue(e.target.value)} />
      </section>

      <section>
        <label>Relevant files <span className="muted">(optional)</span></label>
        <div className="files">
          {files.map((f, i) => (
            <div className="row" key={i}>
              <input
                placeholder={i === 0 ? "Navbar.jsx, src/styles.css…" : "Another file"}
                value={f}
                onChange={(e) => setFiles(files.map((x, j) => (j === i ? e.target.value : x)))}
              />
              {files.length > 1 && (
                <button className="link" onClick={() => setFiles(files.filter((_, j) => j !== i))}>Remove</button>
              )}
            </div>
          ))}
          <button className="link add" onClick={() => setFiles([...files, ""])}>+ Add file</button>
        </div>
      </section>

      <div className="row">
        <button className="primary" disabled={busy} onClick={analyze}>
          {phase === "analyzing" ? "Analyzing…" : "Analyze Repository"}
        </button>
        <span className="muted small">{busy ? progress : "⌘↩ to analyze · read-only, nothing is run or changed"}</span>
      </div>

      {error && <div className="banner error-banner">{error}</div>}

      {analysis && <Results analysis={analysis} />}

      {(phase === "writing" || reply) && (
        <section className="card">
          <label>Reply</label>
          <textarea rows={6} value={reply} readOnly={phase === "writing"} onChange={(e) => setReply(e.target.value)} />
          <div className="row spread">
            <div className="row">
              <button className="primary" disabled={busy || !reply.trim()} onClick={paste}>Paste Reply</button>
              <button disabled={busy || !reply.trim()} onClick={copy}>{copied ? "Copied" : "Copy"}</button>
              <button disabled={busy || !analysis} onClick={writeReply}>Rewrite</button>
            </div>
            <span className="muted small">⌘⇧↩ to paste · never sends</span>
          </div>
        </section>
      )}
    </div>
  );
}

function Results({ analysis }: { analysis: Analysis }) {
  const found = analysis.status === "found";
  return (
    <section className="card results">
      <div className="row spread">
        <strong>{found ? "Issue found" : "No confident diagnosis"}</strong>
        <div className="row">
          <span className="tag">{analysis.mode}</span>
          <span className={`confidence ${analysis.confidence}`}>{CONFIDENCE_LABEL[analysis.confidence]}</span>
        </div>
      </div>
      {analysis.summary && <p>{analysis.summary}</p>}
      {analysis.findings.map((f, i) => <FindingView key={i} finding={f} />)}
      {!found && analysis.missingInfo && (
        <p className="muted">The reply will ask the student for: {analysis.missingInfo}</p>
      )}
      <details>
        <summary className="muted small">
          Read {analysis.examined.length} file{analysis.examined.length === 1 ? "" : "s"} from {analysis.repo}
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
    </section>
  );
}

function FindingView({ finding: f }: { finding: Finding }) {
  const location = f.lineStart ? `${f.file}:${f.lineStart}${f.lineEnd && f.lineEnd !== f.lineStart ? `-${f.lineEnd}` : ""}` : f.file;
  return (
    <div className="finding">
      {f.url ? (
        <button className="link mono" onClick={() => api.openGithubUrl(f.url!)}>{location}</button>
      ) : (
        <span className="mono">{location}</span>
      )}
      {f.snippet && (
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
      )}
      <div><span className="muted">Likely cause:</span> {f.cause}</div>
      {f.fix && <div><span className="muted">Suggested fix:</span> {f.fix}</div>}
    </div>
  );
}
