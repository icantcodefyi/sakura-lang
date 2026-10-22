"use client";

import { Link2, Loader2, Play, Square } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { TokenSpans, type TokenLine } from "@/components/token-spans";
import type { Example } from "@/lib/play-examples";
import { decodeCode, encodeCode } from "@/lib/play-link";

/** The program's file in the compiler's snapshot (compiler/crates/sakura_playground). */
const URI = "file:///play/main.sak";
const TIME_LIMIT_MS = 5000;
/** Services with no browser host: refused before `main` starts, as native refuses them. */
const REFUSED = new Set(["Http", "Serve", "Sql", "Llm", "WebSocket", "Worker"]);

type Span = { uri: string; start_byte: number; end_byte: number; role: string };
type Diag = { code: string; severity: string; message: string; spans: Span[] };
type Answer = {
  version: string;
  human: string;
  diagnostics: Diag[];
  errors: boolean;
  effects: string[] | null;
  files: [string, string][] | null;
};
type Problem = Diag & { from: number; to: number; line: number; col: number };
type Chunk = { stream: "out" | "err"; text: string };
type Status =
  | "ok"
  | "failed"
  | "defect"
  | "errors"
  | "refused"
  | "timeout"
  | "output"
  | "stopped"
  | "unsupported";

const STATUS: { [S in Status]: { label: string; tone: "ok" | "fail" | "muted" } } = {
  ok: { label: "exit 0", tone: "ok" },
  failed: { label: "exit 1 · main failed", tone: "fail" },
  defect: { label: "exit 70 · defect", tone: "fail" },
  errors: { label: "exit 1 · compile errors", tone: "fail" },
  refused: { label: "exit 75 · not available", tone: "fail" },
  timeout: { label: `stopped · ${TIME_LIMIT_MS / 1000} s limit`, tone: "fail" },
  output: { label: "stopped · 1 MiB output limit", tone: "fail" },
  stopped: { label: "stopped", tone: "muted" },
  unsupported: { label: "not supported here", tone: "muted" },
};

/** UTF-16 index of each UTF-8 byte offset in `text` (the records count bytes). */
function byteIndex(text: string): (byte: number) => number {
  // oxlint-disable-next-line no-control-regex
  if (/^[\x00-\x7f]*$/.test(text)) return (b) => Math.min(b, text.length);
  const at: number[] = [];
  for (let i = 0; i < text.length; i++) {
    const c = text.codePointAt(i)!;
    const n = c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
    for (let k = 0; k < n; k++) at.push(i);
    if (c >= 0x10000) i++;
  }
  at.push(text.length);
  return (b) => at[Math.min(b, at.length - 1)];
}

function problemsOf(code: string, records: Diag[]): Problem[] {
  const index = byteIndex(code);
  return records.map((r) => {
    const span = r.spans.find((s) => s.uri === URI && s.role === "primary");
    const from = span ? index(span.start_byte) : 0;
    const to = span ? Math.max(index(span.end_byte), from + 1) : 0;
    const before = code.slice(0, from);
    const line = before.split("\n").length;
    return { ...r, from, to, line, col: from - before.lastIndexOf("\n") };
  });
}

/** The compiler worker (public/play/compiler.js), one per page, started on mount. */
function useCompiler() {
  const worker = useRef<Worker | null>(null);
  const waiting = useRef(
    new Map<number, (m: { result?: Answer; error?: string; ms?: number }) => void>(),
  );
  const next = useRef(0);
  const [state, setState] = useState<"loading" | "ready" | "failed">("loading");

  useEffect(() => {
    const w = new Worker("/play/compiler.js", { type: "module" });
    worker.current = w;
    w.addEventListener("message", ({ data }) => {
      if (data.type === "ready") setState("ready");
      else if (data.type === "failed") setState("failed");
      else {
        waiting.current.get(data.id)?.(data);
        waiting.current.delete(data.id);
      }
    });
    w.addEventListener("error", () => setState("failed"));
    return () => w.terminate();
  }, []);

  const compile = useCallback(
    (source: string, emit: boolean) =>
      new Promise<{ result?: Answer; error?: string; ms?: number }>((resolve) => {
        const id = ++next.current;
        waiting.current.set(id, resolve);
        worker.current?.postMessage({ id, source, emit });
      }),
    [],
  );
  return { state, compile };
}

/**
 * Runs emitted files in public/play/sandbox.html: an opaque-origin iframe whose worker has a
 * virtual clock, seeded random and no network. Removing the iframe ends the run.
 */
function runInSandbox(
  files: [string, string][],
  write: (c: Chunk) => void,
  done: (s: Status) => void,
): () => void {
  const frame = document.createElement("iframe");
  frame.setAttribute("sandbox", "allow-scripts");
  frame.setAttribute("aria-hidden", "true");
  frame.tabIndex = -1;
  frame.title = "Program sandbox";
  frame.style.display = "none";
  let timer: ReturnType<typeof setTimeout> | undefined;
  const stop = (s?: Status) => {
    clearTimeout(timer);
    window.removeEventListener("message", onMessage);
    frame.remove();
    if (s) done(s);
  };
  function onMessage(e: MessageEvent) {
    if (e.source !== frame.contentWindow) return;
    const m = e.data;
    if (m.type === "ready") {
      frame.contentWindow?.postMessage({ files }, "*");
      timer = setTimeout(() => stop("timeout"), TIME_LIMIT_MS);
    } else if (m.type === "write")
      write({ stream: m.stream === "err" ? "err" : "out", text: String(m.text) });
    else if (m.type === "done") stop(m.status in STATUS ? m.status : "unsupported");
  }
  window.addEventListener("message", onMessage);
  frame.src = "/play/sandbox.html";
  document.body.append(frame);
  return () => stop();
}

const layer =
  "m-0 py-3 pr-6 pl-3 font-mono text-[16px] leading-[1.6] whitespace-pre [tab-size:2] sm:text-[13.5px] [grid-area:1/1]";

export function Playground({
  examples,
  initialTokens,
}: {
  examples: Example[];
  initialTokens: TokenLine[];
}) {
  const [code, setCode] = useState(examples[0]?.code ?? "");
  const [example, setExample] = useState(examples[0]?.name ?? "");
  const [tokens, setTokens] = useState<TokenLine[] | null>(initialTokens);
  const tokenize = useRef<((code: string) => TokenLine[]) | null>(null);
  const latest = useRef(code);
  const { state: compilerState, compile } = useCompiler();
  const [check, setCheck] = useState<{ answer: Answer; code: string; ms: number } | null>(null);
  const [output, setOutput] = useState<Chunk[]>([]);
  const [status, setStatus] = useState<Status | null>(null);
  const [running, setRunning] = useState(false);
  const [tab, setTab] = useState<"output" | "problems">("output");
  const [caret, setCaret] = useState(0);
  const [shared, setShared] = useState(false);
  const stopRun = useRef<(() => void) | null>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const escaped = useRef(false);

  // The highlighter loads after hydration; until then edited text shows unhighlighted.
  useEffect(() => {
    let live = true;
    void import("./highlight").then(async (m) => {
      const t = await m.loadTokenizer();
      if (!live) return;
      tokenize.current = t;
      setTokens(t(latest.current));
    });
    return () => {
      live = false;
    };
  }, []);

  const edit = useCallback((next: string) => {
    latest.current = next;
    setCode(next);
    setTokens(tokenize.current?.(next) ?? null);
  }, []);

  // A link's program replaces the example; it is never run until the reader presses Run.
  useEffect(() => {
    const load = () =>
      void decodeCode(window.location.hash).then((c) => {
        if (c === null) return;
        edit(c);
        setExample("");
      });
    load();
    window.addEventListener("hashchange", load);
    return () => window.removeEventListener("hashchange", load);
  }, [edit]);

  // `sakura check` as the reader types.
  useEffect(() => {
    if (compilerState !== "ready") return;
    let live = true;
    const t = setTimeout(async () => {
      const { result, ms } = await compile(code, false);
      if (live && result) setCheck({ answer: result, code, ms: ms ?? 0 });
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [code, compile, compilerState]);

  // The last check's problems stay up while the next one runs, so underlines do not flicker.
  const problems = useMemo(
    () => (check ? problemsOf(check.code, check.answer.diagnostics) : []),
    [check],
  );
  const errorCount = problems.filter((p) => p.severity === "error").length;

  const run = useCallback(async () => {
    if (running) {
      stopRun.current?.();
      stopRun.current = null;
      setRunning(false);
      setStatus("stopped");
      return;
    }
    setRunning(true);
    setStatus(null);
    setOutput([]);
    setTab("output");
    const { result, error } = await compile(code, true);
    const finish = (s: Status, chunks: Chunk[] = []) => {
      setOutput((o) => [...o, ...chunks]);
      setStatus(s);
      setRunning(false);
    };
    if (!result) return finish("unsupported", [{ stream: "err", text: `${error}\n` }]);
    setCheck({ answer: result, code, ms: 0 });
    if (result.errors || !result.files) {
      return finish("errors", [{ stream: "err", text: `${result.human}\n` }]);
    }
    const refused = (result.effects ?? []).filter((e) => REFUSED.has(e));
    if (refused.length > 0) {
      return finish("refused", [
        { stream: "err", text: `\`${refused[0]}\` is not available on target \`browser\`\n` },
        {
          stream: "err",
          text: `\nThe playground runs in your browser with no network, database or model access, so \`main\` cannot use ${refused.map((e) => `\`${e}\``).join(", ")}. Install Sakura Lang and run it with \`sakura run\`, or handle the effect inside \`main\` (for example with \`Http.scripted\`).\n`,
        },
      ]);
    }
    stopRun.current = runInSandbox(
      result.files,
      (c) => setOutput((o) => [...o, c]),
      (s) => {
        stopRun.current = null;
        finish(s);
      },
    );
  }, [code, compile, running]);

  useEffect(() => () => stopRun.current?.(), []);

  async function share() {
    const hash = await encodeCode(code);
    window.history.replaceState(null, "", `/play${hash}`);
    try {
      await navigator.clipboard.writeText(window.location.href);
    } catch {
      // The link is in the address bar either way.
    }
    setShared(true);
    setTimeout(() => setShared(false), 1600);
  }

  function choose(name: string) {
    const ex = examples.find((e) => e.name === name);
    if (!ex) return;
    setExample(name);
    edit(ex.code);
    setOutput([]);
    setStatus(null);
    if (window.location.hash) window.history.replaceState(null, "", "/play");
  }

  function jump(p: Problem) {
    const ta = textarea.current;
    if (!ta) return;
    ta.focus({ preventScroll: true });
    ta.setSelectionRange(p.from, p.to);
    setCaret(p.from);
    const lineHeight = parseFloat(getComputedStyle(ta).lineHeight) || 20;
    scroller.current?.scrollTo({ top: Math.max(0, (p.line - 3) * lineHeight) });
  }

  // Tab indents (Escape first lets Tab leave the editor), Enter keeps the line's indent, and
  // Ctrl/Cmd+Enter runs. execCommand keeps the browser's undo history.
  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    const wasEscaped = escaped.current;
    escaped.current = e.key === "Escape";
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void run();
      return;
    }
    const ta = e.currentTarget;
    const insert = (text: string) => {
      e.preventDefault();
      // oxlint-disable-next-line typescript/no-deprecated
      if (!document.execCommand("insertText", false, text)) {
        ta.setRangeText(text, ta.selectionStart, ta.selectionEnd, "end");
        edit(ta.value);
      }
    };
    if (e.key === "Tab" && !e.shiftKey && !wasEscaped) insert("  ");
    else if (e.key === "Enter" && !e.shiftKey && !e.altKey) {
      const start = ta.value.lastIndexOf("\n", ta.selectionStart - 1) + 1;
      const indent = /^[ \t]*/.exec(ta.value.slice(start))![0];
      const open = /[{([]\s*$/.test(ta.value.slice(start, ta.selectionStart)) ? "  " : "";
      insert(`\n${indent}${open}`);
    }
  }

  const lines = code.split("\n");
  const severityAt = useMemo(() => {
    const marks = new Int8Array(code.length + 1);
    for (const p of problems) {
      const s = p.severity === "error" ? 2 : 1;
      for (let i = p.from; i < p.to && i <= code.length; i++) marks[i] = Math.max(marks[i], s);
    }
    return marks;
  }, [code, problems]);
  const problemLines = useMemo(() => {
    const m = new Map<number, number>();
    for (const p of problems)
      m.set(p.line, Math.max(m.get(p.line) ?? 0, p.severity === "error" ? 2 : 1));
    return m;
  }, [problems]);
  const caretLine = code.slice(0, caret).split("\n").length;
  const atCaret =
    problems.find((p) => caret >= p.from && caret <= p.to) ??
    problems.find((p) => p.line === caretLine);

  // The underline layer: the text in runs of equal severity, transparent, so only the wavy
  // underlines show beneath the highlighted text.
  const marked: { text: string; s: number }[] = [];
  for (let i = 0; i < code.length;) {
    const s = severityAt[i];
    let j = i + 1;
    while (j < code.length && severityAt[j] === s) j++;
    marked.push({ text: code.slice(i, j), s });
    i = j;
  }

  const compilerNote =
    compilerState === "loading"
      ? "Loading the compiler…"
      : compilerState === "failed"
        ? "The compiler did not load"
        : check
          ? `Sakura Lang ${check.answer.version} · checked in ${check.ms} ms`
          : "Checking…";

  return (
    <div className="flex min-h-0 flex-1 flex-col lg:h-[calc(100dvh-3.5rem)] lg:flex-none">
      <div className="flex h-14 shrink-0 items-center gap-2 border-b border-border px-3 sm:px-5">
        <label htmlFor="play-example" className="sr-only">
          Example
        </label>
        <select
          id="play-example"
          value={example}
          onChange={(e) => choose(e.target.value)}
          className="h-9 min-w-0 flex-1 cursor-pointer rounded-md border border-border bg-background px-2.5 font-mono text-[13px] text-foreground transition-colors hover:border-strong sm:max-w-56 sm:flex-none"
        >
          <option value="" disabled>
            Your program
          </option>
          {examples.map((e) => (
            <option key={e.name} value={e.name}>
              {e.name}
            </option>
          ))}
        </select>
        <p
          aria-live="polite"
          className="hidden min-w-0 flex-1 truncate px-2 font-mono text-[12px] text-muted-foreground md:block"
        >
          {compilerNote}
        </p>
        <button
          type="button"
          onClick={() => void share()}
          aria-live="polite"
          aria-label={shared ? "Link copied" : "Copy a link to this program"}
          data-checked={shared ? "" : undefined}
          className="blossom-button ml-auto grid size-9 shrink-0 place-items-center rounded-md border border-border text-muted-foreground transition-colors hover:border-strong hover:text-foreground"
        >
          <Link2 className="size-4" />
        </button>
        <button
          type="button"
          onClick={() => void run()}
          disabled={compilerState !== "ready"}
          title="Run (Ctrl or ⌘ + Enter)"
          className="inline-flex h-9 w-[7.25rem] shrink-0 items-center justify-center gap-2 rounded-md bg-primary px-3 text-[14px] font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
        >
          {running ? (
            <Square className="size-3.5 fill-current" />
          ) : compilerState === "loading" ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <Play className="size-3.5 fill-current" />
          )}
          {running ? "Stop" : "Run"}
        </button>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)] lg:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)] lg:grid-rows-[minmax(0,1fr)]">
        <section aria-label="Editor" className="flex min-h-0 flex-col border-border lg:border-r">
          <div
            ref={scroller}
            className="relative h-[52dvh] min-h-0 overflow-auto bg-code lg:h-auto lg:flex-1"
          >
            <div className="grid min-h-full min-w-max grid-cols-[auto_1fr]">
              <div
                aria-hidden
                className="sticky left-0 z-10 border-r border-border/60 bg-code py-3 pr-2.5 pl-3 text-right font-mono text-[16px] leading-[1.6] text-faint tabular-nums select-none sm:text-[13.5px]"
              >
                {lines.map((_, i) => (
                  <div
                    key={i}
                    className={
                      problemLines.get(i + 1) === 2
                        ? "text-fail"
                        : problemLines.get(i + 1) === 1
                          ? "text-sakura-text"
                          : undefined
                    }
                  >
                    {i + 1}
                  </div>
                ))}
              </div>
              <div className="grid">
                <pre aria-hidden className={`${layer} text-foreground`}>
                  {tokens
                    ? tokens.map((line, i) => (
                        <span key={i}>
                          <TokenSpans line={line} />
                          {"\n"}
                        </span>
                      ))
                    : code}{" "}
                </pre>
                <pre aria-hidden className={`${layer} text-transparent`}>
                  {marked.map((m, i) =>
                    m.s === 0 ? (
                      m.text
                    ) : (
                      <span
                        key={i}
                        className={`underline decoration-wavy decoration-[1.5px] underline-offset-[5px] ${m.s === 2 ? "decoration-fail" : "decoration-sakura-text"}`}
                      >
                        {m.text}
                      </span>
                    ),
                  )}{" "}
                </pre>
                <textarea
                  ref={textarea}
                  value={code}
                  onChange={(e) => {
                    edit(e.target.value);
                    setExample("");
                    setCaret(e.target.selectionStart);
                  }}
                  onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
                  onKeyDown={onKeyDown}
                  // Sized to the text by the layers beneath, it never scrolls itself; a frame
                  // before they grow it could, and would then sit out of line with them.
                  onScroll={(e) => {
                    e.currentTarget.scrollTop = 0;
                    e.currentTarget.scrollLeft = 0;
                  }}
                  wrap="off"
                  spellCheck={false}
                  autoCapitalize="off"
                  autoComplete="off"
                  autoCorrect="off"
                  aria-label="Sakura Lang program"
                  aria-describedby="play-keys"
                  className={`${layer} resize-none overflow-hidden bg-transparent text-transparent caret-foreground outline-none selection:bg-sakura/30`}
                />
              </div>
            </div>
          </div>
          <button
            type="button"
            onClick={() => (atCaret ? jump(atCaret) : setTab("problems"))}
            className="flex h-9 shrink-0 items-center gap-2 border-t border-border px-3 text-left font-mono text-[12px] text-muted-foreground sm:px-5"
          >
            {atCaret ? (
              <>
                <span className={atCaret.severity === "error" ? "text-fail" : "text-sakura-text"}>
                  {atCaret.code}
                </span>
                <span className="truncate">{atCaret.message}</span>
              </>
            ) : (
              <span className="truncate">
                {problems.length === 0
                  ? check
                    ? "No problems"
                    : " "
                  : `${problems.length} ${problems.length === 1 ? "problem" : "problems"}`}
              </span>
            )}
          </button>
          <p id="play-keys" className="sr-only">
            Tab inserts two spaces; press Escape, then Tab, to leave the editor. Control or Command
            and Enter runs the program.
          </p>
        </section>

        <section
          aria-label="Output"
          className="flex min-h-[40dvh] flex-col border-t border-border lg:min-h-0 lg:border-t-0"
        >
          <div
            role="tablist"
            className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2 sm:px-3"
          >
            {(["output", "problems"] as const).map((t) => (
              <button
                key={t}
                type="button"
                role="tab"
                aria-selected={tab === t}
                onClick={() => setTab(t)}
                className={`rounded-md px-2.5 py-1 text-[13px] transition-colors ${tab === t ? "bg-muted text-foreground" : "text-muted-foreground hover:text-foreground"}`}
              >
                {t === "output" ? "Output" : "Problems"}
                {t === "problems" ? (
                  <span
                    className={`ml-1.5 inline-block min-w-4 font-mono text-[11px] tabular-nums ${errorCount ? "text-fail" : ""}`}
                  >
                    {problems.length}
                  </span>
                ) : null}
              </button>
            ))}
            <span
              aria-live="polite"
              className={`ml-auto truncate font-mono text-[12px] ${status ? (STATUS[status].tone === "ok" ? "text-ok" : STATUS[status].tone === "fail" ? "text-fail" : "text-muted-foreground") : "text-muted-foreground"}`}
            >
              {running ? "running…" : status ? STATUS[status].label : ""}
            </span>
          </div>
          <div role="tabpanel" className="min-h-0 flex-1 overflow-auto">
            {tab === "output" ? (
              <pre className="m-0 px-4 py-3 font-mono text-[13px] leading-[1.6] whitespace-pre-wrap [overflow-wrap:anywhere] sm:px-5">
                {output.length === 0 && !running ? (
                  <span className="text-muted-foreground">
                    {status ? "(no output)" : "Press Run to compile and run main."}
                  </span>
                ) : (
                  output.map((c, i) => (
                    <span key={i} className={c.stream === "err" ? "text-fail" : undefined}>
                      {c.text}
                    </span>
                  ))
                )}
              </pre>
            ) : (
              <div className="px-2 py-2 sm:px-3">
                {problems.length > 0 ? (
                  <ul className="mb-2">
                    {problems.map((p, i) => (
                      <li key={i}>
                        <button
                          type="button"
                          onClick={() => jump(p)}
                          className="flex w-full items-baseline gap-2.5 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-muted"
                        >
                          <span
                            className={`shrink-0 font-mono text-[12px] ${p.severity === "error" ? "text-fail" : "text-sakura-text"}`}
                          >
                            {p.code}
                          </span>
                          <span className="min-w-0 flex-1">{p.message}</span>
                          <span className="shrink-0 font-mono text-[11.5px] text-muted-foreground tabular-nums">
                            {p.line}:{p.col}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : null}
                <p className="px-2 pt-1 font-mono text-[11.5px] text-muted-foreground">
                  $ sakura check main.sak
                </p>
                <pre className="m-0 px-2 py-2 font-mono text-[12.5px] leading-[1.6] whitespace-pre-wrap [overflow-wrap:anywhere]">
                  {check?.answer.human ?? "…"}
                </pre>
              </div>
            )}
          </div>
        </section>
      </div>

      <p className="shrink-0 border-t border-border px-3 py-2.5 text-[12px] leading-relaxed text-muted-foreground sm:px-5">
        Runs in your browser: the compiler is WebAssembly, the program a sandboxed worker with a{" "}
        {TIME_LIMIT_MS / 1000} s limit. Console prints here; Clock is virtual from 2026-01-01;
        Random is seeded; Fs is an empty in-memory directory. Http, Serve, Sql, Llm, WebSocket and
        Worker are not available, and <code className="font-mono">sakura verify</code> needs Z3,
        which does not run here; contracts are still checked at run time.
      </p>
    </div>
  );
}
