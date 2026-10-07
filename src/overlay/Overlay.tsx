/**
 * Windows only: the status pill ("hud") and the rewrite bar ("bar"). macOS
 * draws both natively. Rust sends the content; the page lays it out and
 * reports its size so the window can be placed and shown without taking focus.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./overlay.css";

type HudContent = { text: string; seq: number };
type BarContent = {
  seq: number;
  mode: number;
  modes: string[];
  buttons: { tag: number; title: string; tip: string }[];
};

function useContent<T>(label: string): T | null {
  const [content, setContent] = useState<T | null>(null);
  useEffect(() => {
    invoke<T | null>("overlay_content", { label }).then((c) => c && setContent(c));
    const unlisten = listen<T>("overlay-content", (e) => setContent(e.payload));
    return () => void unlisten.then((f) => f());
  }, [label]);
  return content;
}

/** Reports the element's size whenever `deps` change. */
function useFit(label: string, deps: unknown[]) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const r = ref.current?.getBoundingClientRect();
    if (r && r.width) invoke("overlay_fit", { label, width: Math.ceil(r.width), height: Math.ceil(r.height) });
  }, deps);
  return ref;
}

export function Hud() {
  const content = useContent<HudContent>("hud");
  const ref = useFit("hud", [content]);
  if (!content) return null;
  return <div ref={ref} className="pill">{content.text}</div>;
}

export function Bar() {
  const content = useContent<BarContent>("bar");
  const [picking, setPicking] = useState(false);
  const ref = useFit("bar", [content, picking]);
  useEffect(() => setPicking(false), [content]);
  if (!content) return null;

  return (
    <div ref={ref} className="pill bar">
      {picking ? (
        content.modes.map((m, i) => (
          <button
            key={m}
            className={i === content.mode ? "on" : ""}
            title={i === content.mode ? "Detected mode" : `Regenerate as ${m}`}
            onClick={() => {
              setPicking(false);
              invoke("bar_mode", { index: i });
            }}
          >
            {m}
          </button>
        ))
      ) : (
        <>
          <button className="mode" title="Detected mode. Pick another to regenerate in that mode." onClick={() => setPicking(true)}>
            {content.modes[content.mode]} ▾
          </button>
          {content.buttons.map((b) => (
            <button key={b.tag} title={b.tip} onClick={() => invoke("bar_click", { tag: b.tag })}>
              {b.title}
            </button>
          ))}
        </>
      )}
    </div>
  );
}
