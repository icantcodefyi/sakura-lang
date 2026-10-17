"use client";

import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import { Seal } from "@/components/seal";
import { TokenSpans, type TokenLine } from "@/components/token-spans";

/*
 * The hero story, in one loop of the agent/compiler conversation:
 * 1. an agent streams `signup` token by token,
 * 2. the checker confirms each effect against the signature as lines land,
 * 3. line 12 performs `Http`, which the signature never allowed: V3001,
 * 4. the agent declines the signature change and deletes the call,
 * 5. the signature is unchanged; its `ensures` stays a runtime check (design §7.2).
 *
 * A narrow window (phones, and the 1024px two-column layout) tells the same story without
 * sideways scrolling: long lines wrap with a hanging
 * indent, margin notes become gutter marks plus a ticker in the status bar, and the seal
 * stamps the status bar instead of overhanging the window.
 */

const BAD_LINE = 12;

type Note = { kind: "ok" | "fail" | "seal"; text: string };

const NOTES: Record<number, Note> = {
  2: { kind: "ok", text: "error union" },
  5: { kind: "seal", text: "4 effects" },
  8: { kind: "ok", text: "Db, Fail" },
  9: { kind: "ok", text: "Random" },
  10: { kind: "ok", text: "Clock" },
  11: { kind: "ok", text: "Db" },
  [BAD_LINE]: { kind: "fail", text: "Http not allowed" },
};

type Phase = "idle" | "typing" | "caught" | "fixing" | "kept";

type State = {
  phase: Phase;
  /** characters streamed so far, over all lines (newlines excluded) */
  typed: number;
  /** characters removed from the bad line */
  deleted: number;
  diagnostic: boolean;
};

type Action = Partial<State>;

const reducer = (s: State, a: Action): State => ({ ...s, ...a });

type Frame = { delay: number; patch: Action };

function buildFrames(lines: TokenLine[]) {
  const frames: Frame[] = [{ delay: 450, patch: { phase: "typing" } }];
  let typed = 0;
  let n = 0;
  lines.forEach((line, li) => {
    // Stream like a model emits: whitespace rides along with the next token.
    let pending = 0;
    for (const t of line) {
      pending += t.text.length;
      if (t.text.trim() === "") continue;
      typed += pending;
      pending = 0;
      n++;
      frames.push({ delay: 26 + ((n * 37) % 41), patch: { typed } });
    }
    if (pending) {
      typed += pending;
      frames.push({ delay: 20, patch: { typed } });
    }
    // A short beat at each line end; the checker reacts to the bad line.
    if (li === BAD_LINE) frames.push({ delay: 420, patch: { phase: "caught" } });
    else frames.push({ delay: line.length === 0 ? 60 : 110, patch: {} });
  });
  frames.push({ delay: 500, patch: { diagnostic: true } });
  const lens = lines.map((l) => l.reduce((a, t) => a + t.text.length, 0));
  const starts = lens.map((_, i) => lens.slice(0, i).reduce((a, b) => a + b, 0));
  const badLen = lens[BAD_LINE];
  frames.push({ delay: 2600, patch: { phase: "fixing" } });
  for (let d = 0; d < badLen; d += 3)
    frames.push({ delay: 16, patch: { deleted: Math.min(badLen, d + 3) } });
  frames.push({ delay: 260, patch: { deleted: badLen, diagnostic: false } });
  frames.push({ delay: 380, patch: { phase: "kept" } });
  return { frames, total: typed, badLen, lens, starts };
}

export function HeroDemo({ lines }: { lines: TokenLine[] }) {
  const { frames, total, badLen, lens, starts } = useMemo(() => buildFrames(lines), [lines]);
  const [state, set] = useReducer(reducer, {
    phase: "idle",
    typed: 0,
    deleted: 0,
    diagnostic: false,
  });
  const root = useRef<HTMLDivElement>(null);
  const run = useRef(0);

  const play = useCallback(() => {
    const id = ++run.current;
    set({ phase: "idle", typed: 0, deleted: 0, diagnostic: false });
    let i = 0;
    const step = () => {
      if (id !== run.current || i >= frames.length) return;
      const f = frames[i++];
      window.setTimeout(() => {
        if (id !== run.current) return;
        set(f.patch);
        step();
      }, f.delay);
    };
    step();
  }, [frames]);

  useEffect(() => {
    const el = root.current;
    const runs = run;
    if (!el) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      set({ phase: "kept", typed: total, deleted: badLen, diagnostic: false });
      return;
    }
    const io = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          io.disconnect();
          play();
        }
      },
      { threshold: 0.35 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      runs.current++; // cancels any pending frames
    };
  }, [play, total, badLen]);

  // Map the global typed counter onto each line.
  const rows = lines.map((line, i) => {
    const shown = Math.max(0, Math.min(lens[i], state.typed - starts[i]));
    const started = state.typed > 0 && state.typed >= starts[i];
    const complete = started && state.typed >= starts[i] + lens[i];
    return { line, shown, started, complete };
  });

  const removed = state.deleted >= badLen;
  const cursorLine = state.phase === "fixing" ? BAD_LINE : rows.findLastIndex((r) => r.started);
  const done = state.phase === "kept";
  const tickerLine = rows.findLastIndex((r, i) => NOTES[i] && r.complete);

  return (
    <div ref={root} className="@container relative">
      <div
        className="overflow-hidden rounded-2xl border border-line bg-code shadow-[0_1px_0_oklch(1_0_0/0.04)_inset,0_30px_80px_-30px_oklch(0.2_0.02_255/0.35)]"
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- animated composite figure; an <img> can't hold it
        role="img"
        aria-label="Animation: an agent writes a Vow function. The compiler checks each effect against the signature, flags an HTTP call the signature does not allow, and the agent removes it so the signature's promise holds."
      >
        {/* window chrome; its right end stays empty for the seal */}
        <div className="flex items-center gap-3 border-b border-line px-4 py-2.5 font-mono text-[11.5px] text-fg-muted">
          <span className="flex gap-1.5" aria-hidden>
            <span className="size-2.5 rounded-full bg-line-strong" />
            <span className="size-2.5 rounded-full bg-line-strong" />
            <span className="size-2.5 rounded-full bg-line-strong" />
          </span>
          src/users.vow
        </div>

        <pre
          aria-hidden
          className="overflow-x-auto py-4 font-mono text-[12px] leading-[1.75] @min-[38rem]:text-[12.25px]"
        >
          <code className="grid content-start @min-[38rem]:min-w-max">
            {rows.map((r, i) => {
              if (i === BAD_LINE && removed) return null;
              const visible = r.started;
              const note = NOTES[i];
              const showNote = note && r.complete && !(i === BAD_LINE && state.phase === "fixing");
              const bad = i === BAD_LINE && (state.phase === "caught" || state.phase === "fixing");
              const upTo = i === BAD_LINE ? Math.max(0, r.shown - state.deleted) : r.shown;
              const proved = i === 6 && done;
              return (
                <span
                  key={i}
                  className={`grid grid-cols-[2rem_minmax(0,1fr)] transition-colors duration-300 @min-[38rem]:grid-cols-[2.25rem_1fr_7.5rem] ${
                    bad ? "bg-fail-wash" : proved ? "bg-ok-wash" : ""
                  } ${visible ? "" : "invisible"}`}
                >
                  <span className="relative pr-2.5 text-right text-fg-faint tabular-nums select-none @min-[38rem]:pr-3">
                    {showNote || proved ? (
                      <GutterMark kind={proved ? "ok" : (note?.kind ?? "ok")} />
                    ) : null}
                    {visible ? i + 1 - (removed && i > BAD_LINE ? 1 : 0) : ""}
                  </span>
                  {/* The invisible full line reserves the final height, so wrapped lines never
                      push the window taller while the agent types. */}
                  <span className="grid pr-3 @min-[38rem]:pr-2">
                    <span className={`invisible col-start-1 row-start-1 ${LINE}`}>
                      <TokenSpans line={r.line} />
                    </span>
                    <span className={`col-start-1 row-start-1 ${LINE}`}>
                      <TokenSpans line={r.line} upTo={upTo} />
                      {cursorLine === i && !done ? <Caret /> : null}
                    </span>
                  </span>
                  <span className="hidden pr-4 @min-[38rem]:block">
                    {proved ? (
                      <MarginNote note={{ kind: "ok", text: "runtime check" }} />
                    ) : showNote ? (
                      <MarginNote note={note} />
                    ) : null}
                  </span>
                </span>
              );
            })}
          </code>
        </pre>

        {/* diagnostic, expands under the code */}
        <div
          className={`grid transition-[grid-template-rows] duration-500 ease-[var(--ease-out)] ${
            state.diagnostic ? "grid-rows-[1fr]" : "grid-rows-[0fr]"
          }`}
        >
          <div className="overflow-hidden">
            <Diagnostic />
          </div>
        </div>

        <StatusBar
          phase={state.phase}
          ticker={
            state.phase === "typing" && tickerLine >= 0
              ? { line: tickerLine + 1, note: NOTES[tickerLine] }
              : undefined
          }
          onReplay={done ? play : undefined}
        />
      </div>
      {/* The seal lands on the window's empty corner (in a narrow window, in the status bar). */}
      <Seal
        className={`pointer-events-none absolute -top-10 -right-3 hidden size-[6.5rem] drop-shadow-[0_6px_10px_oklch(0.45_0.12_70/0.35)] transition-[opacity,transform] @min-[38rem]:block lg:-right-7 ${
          done
            ? "rotate-[-14deg] scale-100 opacity-100 duration-[420ms] ease-[var(--ease-out)]"
            : "rotate-[-30deg] scale-[1.6] opacity-0 duration-0"
        }`}
      />
    </div>
  );
}

/** In a narrow window, wrap with a hanging indent; from 38rem up, one unbroken line. */
const LINE =
  "whitespace-pre-wrap pl-[2ch] -indent-[2ch] [overflow-wrap:anywhere] @min-[38rem]:whitespace-pre @min-[38rem]:pl-0 @min-[38rem]:indent-0";

/** A narrow window has no margin for notes; a colored tick in the gutter stands in for them. */
function GutterMark({ kind }: { kind: Note["kind"] }) {
  const tone = kind === "fail" ? "bg-fail" : kind === "seal" ? "bg-seal" : "bg-ok";
  return (
    <span
      aria-hidden
      className={`absolute inset-y-[0.35em] left-0 w-[3px] rounded-r-full @min-[38rem]:hidden motion-safe:animate-[note-in_260ms_var(--ease-out)_both] ${tone}`}
    />
  );
}

function Caret() {
  return (
    <span
      aria-hidden
      className="ml-px inline-block h-[1.15em] w-[0.5ch] translate-y-[0.2em] bg-seal motion-safe:animate-[caret_1s_steps(1)_infinite]"
    />
  );
}

function MarginNote({ note }: { note: Note }) {
  const tone =
    note.kind === "fail"
      ? "text-fail border-fail/40"
      : note.kind === "seal"
        ? "text-seal-text border-seal/50"
        : "text-ok border-ok/35";
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border px-2 py-px font-sans text-[11px] leading-[1.6] font-medium whitespace-nowrap motion-safe:animate-[note-in_260ms_var(--ease-out)_both] ${tone}`}
    >
      {note.kind === "fail" ? <Cross /> : <Tick />}
      {note.text}
    </span>
  );
}

function Tick() {
  return (
    <svg viewBox="0 0 12 12" className="size-2.5" aria-hidden>
      <path
        d="M2.5 6.3 5 8.6l4.5-5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function Cross() {
  return (
    <svg viewBox="0 0 12 12" className="size-2.5" aria-hidden>
      <path
        d="m3 3 6 6M9 3 3 9"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </svg>
  );
}

function Diagnostic() {
  return (
    <div className="border-t border-line px-4 py-3.5 font-mono text-[11.5px] leading-relaxed @min-[38rem]:px-5">
      <p className="text-fg">
        <span className="font-semibold text-fail">error V3001</span>{" "}
        <span className="text-fg-muted">src/users.vow:12:11</span>
      </p>
      <p className="mt-1 font-sans text-[13px] text-fg">
        function <code className="font-mono">signup</code> performs{" "}
        <code className="font-mono text-seal-text">Http</code>, which its signature does not allow.
      </p>
      <ul className="mt-3 grid gap-1.5">
        <li className="flex flex-wrap items-baseline gap-x-2">
          <span className="rounded border border-line px-1.5 text-[10.5px] text-fg-muted">
            interface_change
          </span>
          <span className="text-fg-muted">add Http to the signature: needs review</span>
        </li>
        <li className="flex flex-wrap items-baseline gap-x-2">
          <span className="rounded border border-ok/40 px-1.5 text-[10.5px] text-ok">agent</span>
          <span className="text-fg">removes the call instead</span>
          <span className="text-seal-text">← keeps the promise</span>
        </li>
      </ul>
    </div>
  );
}

const STATUS: Record<Phase, { text: string; tone: string }> = {
  idle: { text: "vow check --json", tone: "text-fg-faint" },
  typing: { text: "agent is writing, checker is following", tone: "text-fg-muted" },
  caught: { text: "1 error: effect not in signature", tone: "text-fail" },
  fixing: { text: "agent edit: remove the call", tone: "text-fg-muted" },
  kept: { text: "0 errors · signature kept", tone: "text-ok" },
};

function StatusBar({
  phase,
  ticker,
  onReplay,
}: {
  phase: Phase;
  /** narrow window only: the latest margin note, which has no margin to sit in */
  ticker?: { line: number; note: Note };
  onReplay?: () => void;
}) {
  const s = STATUS[phase];
  const kept = phase === "kept";
  return (
    <div className="flex min-h-10 items-center justify-between gap-3 border-t border-line bg-bg-sunk/60 px-4 font-mono text-[11.5px]">
      <span className="flex min-w-0 items-center gap-2" aria-live="polite">
        {kept ? (
          <Seal className="-my-2 -ml-1 size-8 shrink-0 motion-reduce:rotate-[-14deg] motion-safe:animate-[stamp_420ms_var(--ease-out)_both] @min-[38rem]:hidden" />
        ) : null}
        <span
          aria-hidden
          className={`size-1.5 shrink-0 rounded-full ${kept ? "bg-ok @max-[38rem]:hidden" : phase === "caught" ? "bg-fail" : "bg-seal motion-safe:animate-pulse"}`}
        />
        <span
          key={phase}
          className={`${s.tone} truncate motion-safe:animate-[note-in_220ms_var(--ease-out)_both] ${ticker ? "@max-[38rem]:hidden" : ""}`}
        >
          {s.text}
        </span>
        {ticker ? (
          <span
            key={ticker.line}
            aria-hidden
            className={`truncate motion-safe:animate-[note-in_220ms_var(--ease-out)_both] @min-[38rem]:hidden ${
              ticker.note.kind === "fail"
                ? "text-fail"
                : ticker.note.kind === "seal"
                  ? "text-seal-text"
                  : "text-ok"
            }`}
          >
            {ticker.note.kind === "fail" ? <Cross /> : <Tick />} {ticker.note.text}{" "}
            <span className="text-fg-faint">on line {ticker.line}</span>
          </span>
        ) : null}
      </span>
      {onReplay ? (
        <button
          type="button"
          onClick={onReplay}
          className="-mr-2 h-10 shrink-0 rounded-md px-3 font-sans text-[12px] text-fg-muted transition-[color,transform] duration-150 hover:text-fg active:scale-[0.97]"
        >
          Replay
        </button>
      ) : null}
    </div>
  );
}
