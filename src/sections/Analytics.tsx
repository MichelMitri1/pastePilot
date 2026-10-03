import { useCallback, useEffect, useState } from "react";
import * as api from "../api";
import type { Count, Dashboard } from "../api";
import { ConfirmButton, NumberField } from "../components/common";
import type { SectionProps } from "./types";

const RANGES = [
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
  { days: 90, label: "90 days" },
  { days: 0, label: "All time" },
];

const dayLabel = (daysAgo: number) => {
  if (daysAgo === 0) return "Today";
  const d = new Date(Date.now() - daysAgo * 86_400_000);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
};

const hours = (minutes: number) =>
  minutes < 60 ? `${Math.round(minutes)} min` : `${(minutes / 60).toFixed(minutes < 600 ? 1 : 0)} h`;

export default function Analytics({ settings, update, flash }: SectionProps) {
  const [range, setRange] = useState(30);
  const [data, setData] = useState<Dashboard | null>(null);
  const [asTable, setAsTable] = useState(false);

  const load = useCallback(() => api.getDashboard(range).then(setData).catch((e) => flash(String(e), true)), [range, flash]);
  useEffect(() => {
    load();
  }, [load, settings.minutesPerReply, settings.minutesPerDebug]);

  if (!data) return null;
  const editedPct = data.sent ? Math.round((data.edited / data.sent) * 100) : null;

  return (
    <div className="viz-root">
      <section className="row spread">
        <div className="segmented-row" role="tablist" aria-label="Time range">
          {RANGES.map((r) => (
            <button key={r.days} role="tab" aria-selected={range === r.days} className={range === r.days ? "on" : ""} onClick={() => setRange(r.days)}>
              {r.label}
            </button>
          ))}
        </div>
        <span className="muted small">Counts only, stored on this Mac. Message text is never part of analytics.</span>
      </section>

      <section className="tiles">
        <Tile label="Replies generated" value={String(data.replies)} />
        <Tile label="Debug cases" value={String(data.debugCases)} />
        <Tile label="Estimated time saved" value={hours(data.minutesSaved)} />
        <Tile
          label="Edited before sending"
          value={editedPct === null ? "–" : `${editedPct}%`}
          note={data.sent ? `of ${data.sent} sent replies` : "appears after you send replies"}
        />
      </section>

      <section className="card">
        <div className="row spread">
          <label>Activity per day</label>
          <div className="row">
            <span className="legend"><i className="swatch s1" /> Replies</span>
            <span className="legend"><i className="swatch s2" /> Debug cases</span>
            <button className="link" onClick={() => setAsTable(!asTable)}>{asTable ? "Show chart" : "Show table"}</button>
          </div>
        </div>
        {asTable ? <DayTable days={data.days} /> : <DayChart days={data.days} />}
      </section>

      <section className="grid">
        <BarList title="Replies by mode" items={data.modes} empty="No replies yet." />
        <BarList title="Common issue types (debugging)" items={data.issueTypes} empty="No debug cases yet." />
        <BarList title="Project types debugged" items={data.projectTypes} empty="No debug cases yet." />
        <BarList title="Rewrites used" items={data.rewriteActions} empty="No rewrites yet." />
      </section>

      <section className="card">
        <label>Time-saved estimate</label>
        <div className="grid">
          <NumberField label="Minutes saved per reply" value={settings.minutesPerReply} min={0} max={60} onChange={(v) => update("minutesPerReply", v)} />
          <NumberField label="Minutes saved per debug case" value={settings.minutesPerDebug} min={0} max={240} onChange={(v) => update("minutesPerDebug", v)} />
        </div>
        <span className="muted small">
          {data.examplesLearned > 0 && `${data.examplesLearned} reply example${data.examplesLearned === 1 ? "" : "s"} learned in this period. `}
          Click Save to apply new estimates.
        </span>
      </section>

      <section className="row end">
        <ConfirmButton label="Reset analytics" onConfirm={() => api.resetAnalytics().then(load).then(() => flash("Analytics reset"))} />
      </section>
    </div>
  );
}

function Tile({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="tile">
      <div className="muted small">{label}</div>
      <div className="tile-value">{value}</div>
      {note && <div className="muted small">{note}</div>}
    </div>
  );
}

/** Stacked daily bars: replies (series 1) under debug cases (series 2), 2px gap, rounded top. */
function DayChart({ days }: { days: Dashboard["days"] }) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(1, ...days.map((d) => d.replies + d.debug));
  const ticks = [max, Math.round(max / 2)].filter((v, i, a) => v > 0 && a.indexOf(v) === i);
  const height = 140;
  const labelEvery = Math.ceil(days.length / 6);
  return (
    <div className="daychart">
      <div className="plot" style={{ height }} onMouseLeave={() => setHover(null)}>
        {ticks.map((t) => (
          <div key={t} className="gridline" style={{ bottom: (t / max) * height }}>
            <span>{t}</span>
          </div>
        ))}
        <div className="bars">
          {days.map((d, i) => {
            const r = (d.replies / max) * height;
            const g = (d.debug / max) * height;
            return (
              <div key={d.daysAgo} className="col" onMouseEnter={() => setHover(i)} aria-label={`${dayLabel(d.daysAgo)}: ${d.replies} replies, ${d.debug} debug cases`}>
                {d.debug > 0 && <div className={`seg s2 ${d.debug > 0 ? "top" : ""}`} style={{ height: g }} />}
                {d.debug > 0 && d.replies > 0 && <div className="gap" />}
                {d.replies > 0 && <div className={`seg s1 ${d.debug === 0 ? "top" : ""}`} style={{ height: r }} />}
              </div>
            );
          })}
        </div>
        {hover !== null && (
          <div className="tooltip" style={{ left: `${((hover + 0.5) / days.length) * 100}%` }}>
            <strong>{dayLabel(days[hover].daysAgo)}</strong>
            <div><i className="swatch s1" /> {days[hover].replies} replies</div>
            <div><i className="swatch s2" /> {days[hover].debug} debug cases</div>
          </div>
        )}
      </div>
      <div className="xlabels">
        {days.map((d, i) => (
          <span key={d.daysAgo}>{i % labelEvery === 0 || i === days.length - 1 ? dayLabel(d.daysAgo) : ""}</span>
        ))}
      </div>
    </div>
  );
}

function DayTable({ days }: { days: Dashboard["days"] }) {
  const rows = days.filter((d) => d.replies || d.debug).slice().reverse();
  if (!rows.length) return <span className="muted">No activity in this period.</span>;
  return (
    <table className="data-table">
      <thead><tr><th>Day</th><th>Replies</th><th>Debug cases</th></tr></thead>
      <tbody>
        {rows.map((d) => (
          <tr key={d.daysAgo}><td>{dayLabel(d.daysAgo)}</td><td>{d.replies}</td><td>{d.debug}</td></tr>
        ))}
      </tbody>
    </table>
  );
}

/** Single-series horizontal bars (one hue), value labels in text color. */
function BarList({ title, items, empty }: { title: string; items: Count[]; empty: string }) {
  const max = Math.max(1, ...items.map((i) => i.count));
  return (
    <div className="card barlist">
      <label>{title}</label>
      {items.length === 0 ? (
        <span className="muted small">{empty}</span>
      ) : (
        items.map((i) => (
          <div key={i.label} className="barrow" title={`${i.label}: ${i.count}`}>
            <span className="clip">{i.label}</span>
            <div className="track"><div className="fill" style={{ width: `${(i.count / max) * 100}%` }} /></div>
            <span className="value">{i.count}</span>
          </div>
        ))
      )}
    </div>
  );
}
