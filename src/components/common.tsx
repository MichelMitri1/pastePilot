import { useEffect, useRef, useState } from "react";

const MODIFIER_CODES = new Set([
  "MetaLeft", "MetaRight", "AltLeft", "AltRight", "ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight",
]);

/** Turns a keydown into the accelerator format the Rust global-shortcut plugin parses ("Alt+R"). */
function shortcutFromEvent(e: React.KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const parts: string[] = [];
  if (e.metaKey) parts.push("Cmd");
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (parts.length === 0) return null; // a bare key would hijack normal typing
  let key = e.code;
  if (key.startsWith("Key")) key = key.slice(3);
  else if (key.startsWith("Digit")) key = key.slice(5);
  parts.push(key);
  return parts.join("+");
}

export const prettyShortcut = (s: string) =>
  s ? s.replace(/Cmd|Super|Command/g, "⌘").replace(/Alt|Option/g, "⌥").replace(/Ctrl|Control/g, "⌃")
    .replace(/Shift/g, "⇧").replace(/\+/g, " ") : "None";

export function ShortcutInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [recording, setRecording] = useState(false);
  return (
    <input
      readOnly
      className={recording ? "shortcut recording" : "shortcut"}
      value={recording ? "Press keys…" : prettyShortcut(value)}
      onFocus={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        e.preventDefault();
        const s = shortcutFromEvent(e);
        if (s) {
          onChange(s);
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

/** Two-step button for destructive actions: first click arms it, second confirms. */
export function ConfirmButton({ label, onConfirm }: { label: string; onConfirm: () => void }) {
  const [armed, setArmed] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <button
      className={armed ? "danger armed" : "danger"}
      onClick={() => {
        if (armed) {
          window.clearTimeout(timer.current);
          setArmed(false);
          onConfirm();
        } else {
          setArmed(true);
          timer.current = window.setTimeout(() => setArmed(false), 3000);
        }
      }}
    >
      {armed ? "Click again to confirm" : label}
    </button>
  );
}

export function Toggle({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: React.ReactNode }) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{children}</span>
    </label>
  );
}

export function NumberField({ label, value, min, max, onChange, hint }: {
  label: string; value: number; min: number; max: number; onChange: (v: number) => void; hint?: string;
}) {
  return (
    <div className="field">
      <label>{label}</label>
      <input
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Math.max(min, Math.min(max, Number(e.target.value) || min)))}
      />
      {hint && <span className="muted small">{hint}</span>}
    </div>
  );
}

/** Lets the user pick a .json/.txt/.md file and returns its text. */
export function FileTextButton({ onText }: { onText: (text: string) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  return (
    <>
      <button onClick={() => ref.current?.click()}>Choose file…</button>
      <input
        ref={ref}
        type="file"
        accept=".json,.txt,.md,.markdown"
        hidden
        onChange={async (e) => {
          const file = e.target.files?.[0];
          if (file) onText(await file.text());
          e.target.value = "";
        }}
      />
    </>
  );
}
