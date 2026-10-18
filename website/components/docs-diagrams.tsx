import { ArrowRight } from "lucide-react";
import { Scopes } from "@/components/landing/authority";

// Flat step diagrams for the docs: a row (column on phones) of code chips joined by arrows.
// Everything is real text rendered on the server, and lib/source.ts prints the same data as
// Markdown for the .md and llms routes, so the two never drift.

// `ok` marks the step where the flow ends well; the note says so in words too.
type Step = { code: string; note: string; tone?: "ok" };
type Row = { label?: string; steps: Step[] };
type Diagram = { caption: string; rows: Row[]; after?: string; column?: boolean };

const DIAGRAMS = {
  EffectFlow: {
    caption:
      "An operation's effect travels up through every caller's signature until a handler takes it.",
    rows: [
      {
        steps: [
          { code: "Clock.now()", note: "an operation performs Clock" },
          { code: "fn greet(…) ! {Console, Clock}", note: "greet's row must list it" },
          { code: "fn main() ! {Console, Clock}", note: "calling greet performs greet's row" },
        ],
      },
    ],
    after:
      "A label missing from a declared row is V3001. A with handler removes the effects it handles from the row.",
  },
  HandlerFlow: {
    caption: "A handler clause runs in place of the operation and returns its result once.",
    rows: [
      {
        steps: [
          { code: "Counter.next()", note: "the body performs an operation" },
          { code: "with memory_counter(0)", note: "the innermost Counter handler takes it" },
          { code: "fn next() { … }", note: "its clause runs in place of the call" },
          { code: "1", note: "the clause's value is the call's result", tone: "ok" },
        ],
      },
    ],
  },
  ContractFlow: {
    caption: "Where each clause of a call to clamp is checked.",
    column: true,
    rows: [
      {
        steps: [
          { code: "clamp(x, lo, hi)", note: "the caller passes arguments" },
          {
            code: "requires ordered",
            note: "checked on entry; a violation is D020, caller blamed",
          },
          { code: "{ body }", note: "runs" },
          {
            code: "ensures bounded",
            note: "checked on normal return; a violation is D021, callee blamed",
          },
          { code: "result", note: "returned to the caller", tone: "ok" },
        ],
      },
    ],
    after:
      "A where refinement is checked when a value enters the type. A scalar literal is decided at compile time when the bounded evaluator can (V5004 on rejection); every other check stays at runtime.",
  },
  AgentLoop: {
    caption: "The agent loop: check, read structured diagnostics, apply fixes by impact, repeat.",
    rows: [
      {
        steps: [
          { code: "fn … { ?body }", note: "write signatures; leave bodies as holes" },
          { code: "sakura check --json", note: "one JSON Lines record per problem" },
          { code: "V3001 + fixes[]", note: "code, byte spans, expected and actual, fixes" },
          { code: "sakura fix", note: "apply by impact class; report signature changes" },
        ],
      },
    ],
    after: "Repeat from sakura check until it is clean, then run sakura test --json.",
  },
  CassetteFlow: {
    caption: "Recording is opt-in; ordinary tests replay the cassette with no network.",
    rows: [
      {
        label: "sakura test --record",
        steps: [
          { code: "Http.send(req)", note: "the test sends a request" },
          { code: "Http.recording", note: "no matching exchange" },
          { code: "outer Http", note: "forwards to a manifest net host" },
          { code: "cassette.jsonl", note: "the exchange is appended under tests/fixtures/" },
        ],
      },
      {
        label: "sakura test",
        steps: [
          { code: "Http.send(req)", note: "the same request" },
          { code: "Http.recording", note: "matches method, URL, headers, body" },
          { code: "cassette.jsonl", note: "the recorded response comes back", tone: "ok" },
        ],
      },
    ],
    after: "Without --record, a miss fails with HttpError pointing to sakura test --record.",
  },
  PipelineFlow: {
    caption: "The compiler pipeline (design §13.1). Each stage feeds the next.",
    column: true,
    rows: [
      {
        steps: [
          { code: "source text", note: "lexed and parsed" },
          { code: "CST", note: "lossless syntax tree; the formatter works here" },
          { code: "HIR", note: "names resolved" },
          { code: "Typed HIR", note: "types, effect rows, holes and contract obligations" },
          { code: "Core", note: "typed ANF with explicit Perform, Handle, Fail and Check" },
          {
            code: "JavaScript",
            note: "CPS with a trampoline; the backend that runs today",
            tone: "ok",
          },
        ],
      },
    ],
    after:
      "WASM (M4) and native code through Cranelift (M5) are planned backends from the same Core; neither is available in the CLI today.",
  },
} satisfies Record<string, Diagram>;

type Name = keyof typeof DIAGRAMS;

function Chip({ step, wide }: { step: Step; wide?: boolean }) {
  // Wrap only between words: a chip never splits `--json` or `memory_counter(0)`.
  const words = step.code.split(" ");
  return (
    <code
      className={`inline-block max-w-full rounded-md border px-2.5 py-1 font-mono text-[12px] ${wide ? "sm:w-full sm:px-2" : "sm:text-[12.5px]"} ${step.tone ? "border-ok/40 bg-ok-wash text-ok" : "border-strong bg-card text-foreground"}`}
    >
      {words.flatMap((w, i) => [
        i > 0 ? " " : null,
        <span key={i} className="whitespace-nowrap">
          {w}
        </span>,
      ])}
    </code>
  );
}

const Arrow = ({ className }: { className: string }) => (
  <ArrowRight aria-hidden className={`size-4 shrink-0 text-faint ${className}`} />
);

/** Steps left to right from `sm` up, top to bottom on phones; `column` keeps long chains vertical. */
function Steps({ steps, column }: { steps: Step[]; column?: boolean }) {
  if (column) {
    return (
      <ol className="grid gap-1">
        {steps.map((s, i) => (
          <li key={s.code} className="grid gap-1">
            {i > 0 && <Arrow className="ms-3 rotate-90" />}
            <p className="grid justify-items-start gap-1 sm:flex sm:items-baseline sm:gap-3">
              <Chip step={s} />
              <span className="text-[13px] text-muted-foreground">{s.note}</span>
            </p>
          </li>
        ))}
      </ol>
    );
  }
  return (
    <ol className="grid gap-1 sm:flex sm:items-start sm:gap-8">
      {steps.map((s, i) => (
        // the arrow sits in the gap on wide screens, so every step gets the same width
        <li key={s.code} className="relative grid min-w-0 justify-items-start gap-1 sm:flex-1">
          {i > 0 && (
            <Arrow className="ms-3 rotate-90 sm:absolute sm:top-2 sm:-left-6 sm:ms-0 sm:rotate-0" />
          )}
          <Chip step={s} wide />
          <span className="text-[13px] leading-snug text-muted-foreground">{s.note}</span>
        </li>
      ))}
    </ol>
  );
}

function Flow({ name }: { name: Name }) {
  const d: Diagram = DIAGRAMS[name];
  return (
    <figure data-diagram className="not-prose my-8 grid gap-5">
      {d.rows.map((r) => (
        <div key={r.label ?? "steps"} className="grid gap-2.5">
          {r.label && <p className="font-mono text-[12px] text-sakura-text">{r.label}</p>}
          <Steps steps={r.steps} column={d.column} />
        </div>
      ))}
      {d.after && <p className="text-[13px] text-muted-foreground">{d.after}</p>}
      <figcaption className="border-t border-border pt-2.5 text-[12.5px] text-faint">
        {d.caption}
      </figcaption>
    </figure>
  );
}

const SCOPES_CAPTION =
  "Scopes of authority for the CLI example in design §15.6, outermost first. Each runs inside the one above it and can only narrow it; the bar is an illustrative share of the host's authority.";

export function CapabilityScopes() {
  return (
    <figure data-diagram className="not-prose my-8">
      <Scopes />
      <figcaption className="mt-3 text-[12.5px] text-faint">{SCOPES_CAPTION}</figcaption>
    </figure>
  );
}

export const EffectFlow = () => <Flow name="EffectFlow" />;
export const HandlerFlow = () => <Flow name="HandlerFlow" />;
export const ContractFlow = () => <Flow name="ContractFlow" />;
export const AgentLoop = () => <Flow name="AgentLoop" />;
export const CassetteFlow = () => <Flow name="CassetteFlow" />;
export const PipelineFlow = () => <Flow name="PipelineFlow" />;

const SCOPES_MARKDOWN = [
  "host: everything the machine can do",
  'sakura.toml [permissions]: fs.read = ["./data"], net = [], the ceiling',
  "main() ! {Console, Cli, Fs, …}: root handlers for these effects only",
  'with Fs.rooted(Dir.subdir(…, "data")): paths resolve inside ./data',
  "word_counts(text) -> Map<String, Int>: pure, no authority at all",
];

/** The diagram as a Markdown quote, for the .md and llms routes. */
function markdown(caption: string, rows: { label?: string; items: string[] }[], after?: string) {
  const body = rows
    .map(
      (r) =>
        `${r.label ? `\`${r.label}\`:\n\n` : ""}${r.items.map((s, i) => `${i + 1}. ${s}`).join("\n")}`,
    )
    .join("\n\n");
  return [`**Diagram:** ${caption}`, body, after]
    .filter(Boolean)
    .join("\n\n")
    .split("\n")
    .map((l) => (l ? `> ${l}` : ">"))
    .join("\n");
}

export const diagramMarkdown: Record<Name | "CapabilityScopes", () => string> = {
  CapabilityScopes: () => markdown(SCOPES_CAPTION, [{ items: SCOPES_MARKDOWN }]),
  ...(Object.fromEntries(
    Object.entries(DIAGRAMS).map(([name, d]: [string, Diagram]) => [
      name,
      () =>
        markdown(
          d.caption,
          d.rows.map((r) => ({
            label: r.label,
            items: r.steps.map((s) => `\`${s.code}\`: ${s.note}`),
          })),
          d.after,
        ),
    ]),
  ) as Record<Name, () => string>),
};
