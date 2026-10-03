/** Line diff (LCS) between the real lines and the proposed fix. */
type Row = { kind: "same" | "del" | "add"; text: string };

export function lineDiff(before: string, after: string): Row[] {
  const a = before.split("\n");
  const b = after.split("\n");
  const dp: number[][] = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--)
    for (let j = b.length - 1; j >= 0; j--)
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  const rows: Row[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      rows.push({ kind: "same", text: a[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) rows.push({ kind: "del", text: a[i++] });
    else rows.push({ kind: "add", text: b[j++] });
  }
  while (i < a.length) rows.push({ kind: "del", text: a[i++] });
  while (j < b.length) rows.push({ kind: "add", text: b[j++] });
  return rows;
}

export default function Diff({ before, after, startLine }: { before: string; after: string; startLine: number | null }) {
  const rows = lineDiff(before, after);
  let line = startLine ?? 1;
  return (
    <pre className="snippet diff" aria-label="Before and after">
      {rows.map((r, i) => {
        const n = r.kind === "add" ? "" : String(line++);
        return (
          <div key={i} className={r.kind}>
            <span className="ln">{n}</span>
            <span className="sign">{r.kind === "del" ? "−" : r.kind === "add" ? "+" : " "}</span>
            {r.text || " "}
          </div>
        );
      })}
    </pre>
  );
}
