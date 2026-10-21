import type { CSSProperties, ReactNode } from "react";
import { Play } from "./play";
import { Section, TextLink } from "./section";
import { WINDOW, WindowBar } from "./window";

// One agent session, recorded from the current compiler in a `sakura new cli app` project with
// `fn count(xs: List<Int>) -> Int { list.len(xs) }` in src/count.sak. Long output is trimmed
// with …; re-record when the record shapes change.
type Line = { text: ReactNode; kind?: "cmd" | "json" | "ok" | "dim" };
const C = ({ children }: { children: ReactNode }) => (
  <span className="text-sakura-text">{children}</span>
);
const SESSION: Line[] = [
  { kind: "cmd", text: "sakura setup --client all --apply" },
  { kind: "dim", text: "  create .claude/skills/sakura/SKILL.md" },
  { kind: "dim", text: "  create .mcp.json" },
  { kind: "dim", text: "  create .codex/config.toml" },
  { kind: "dim", text: "  create .cursor/mcp.json   …" },
  { kind: "cmd", text: "sakura check --json" },
  {
    kind: "json",
    text: (
      <>
        {'{"kind":"diagnostic","code":'}
        <C>&quot;V1004&quot;</C>
        {',"message":"cannot find `list`; `List` exists but names differ in case …",'}
      </>
    ),
  },
  {
    kind: "json",
    text: (
      <>
        {' "fixes":[{"title":"Use `List`","applicability":'}
        <C>&quot;machine_applicable&quot;</C>
        {',"impact":"local", …}]}'}
      </>
    ),
  },
  { kind: "cmd", text: "sakura fix" },
  {
    kind: "ok",
    text: "f:db8db68ac7c828f3.0  V1004  src/count.sak:1:34  Use `List`  [machine_applicable, local]  applied",
  },
  { kind: "cmd", text: "sakura test" },
  { kind: "ok", text: "1 passed, 0 failed, 0 skipped (seed 0)" },
];

// The stdio tools of `sakura mcp serve` (docs/agents/mcp).
const TOOLS = [
  "compiler_info",
  "check",
  "symbol_query",
  "hole_context",
  "docs_query",
  "explain_diagnostic",
  "format",
  "check_patch",
  "apply_patch",
  "test",
];

const FACTS: { title: string; body: ReactNode; href: string }[] = [
  {
    title: "Instructions in the project",
    body: (
      <>
        <code className="font-mono">sakura new</code> writes{" "}
        <code className="font-mono">AGENTS.md</code> and a{" "}
        <code className="font-mono">CLAUDE.md</code> that imports it.
      </>
    ),
    href: "/docs/quickstart",
  },
  {
    title: "A skill, and docs as Markdown",
    body: (
      <>
        The <code className="font-mono">sakura</code> skill for each client; every docs page is also
        Markdown, indexed by <code className="font-mono">/llms.txt</code>.
      </>
    ),
    href: "/docs/agents/llms-txt",
  },
  {
    title: "Fixes that state their reach",
    body: (
      <>
        <code className="font-mono">sakura fix</code> applies the machine-applicable ones and never
        a <code className="font-mono">contract_change</code> or{" "}
        <code className="font-mono">authority_change</code>.
      </>
    ),
    href: "/docs/agents/diagnostics",
  },
  {
    title: "Tests an agent can rerun",
    body: "Tests deny the network; HTTP and LLM exchanges replay from cassettes in tests/fixtures.",
    href: "/docs/agents/cassettes",
  },
];

/**
 * The agent's side of the loop: a recorded session that plays once, line by line, when it scrolls
 * into view (CSS `term-in`, opacity and transform; every line is laid out from the start), and the
 * MCP server between the agent and the compiler, with requests and answers moving along it.
 */
export function Agents() {
  return (
    <Section
      id="agents"
      ask="How does it work with my coding agent?"
      title="One command wires your agent to the compiler."
      lede={
        <>
          Claude Code, Codex and Cursor get the MCP server and the project skill. From then on the
          agent asks the compiler instead of guessing.{" "}
          <TextLink href="/docs/agents">Agent docs</TextLink>
        </>
      }
    >
      <div className="grid grid-cols-[minmax(0,1fr)] gap-x-14 gap-y-12 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
        <Play className="min-w-0">
          <figure className={WINDOW}>
            <WindowBar className="justify-between sm:px-4">
              <span>terminal</span>
              <span className="text-muted-foreground">an agent&apos;s session</span>
            </WindowBar>
            <pre className="overflow-x-auto bg-code py-4 font-mono text-[11.5px] leading-[1.8] sm:text-[12px]">
              <code className="grid min-w-max px-4 sm:px-5">
                {SESSION.map((l, i) => (
                  <span
                    key={i}
                    className={`term-in block ${l.kind === "cmd" ? "mt-2 text-foreground first:mt-0" : l.kind === "ok" ? "text-ok" : "text-muted-foreground"}`}
                    style={{ "--term-i": i } as CSSProperties}
                  >
                    {l.kind === "cmd" ? (
                      <span aria-hidden className="text-faint select-none">
                        ${" "}
                      </span>
                    ) : null}
                    {l.text}
                  </span>
                ))}
              </code>
            </pre>
            <figcaption className="border-t border-border px-4 py-2.5 text-[12.5px] text-muted-foreground sm:px-5">
              Recorded from the current compiler; long output trimmed with …
            </figcaption>
          </figure>
        </Play>

        <div className="grid content-start gap-10">
          <McpFigure />
          <ul className="grid border-t border-border">
            {FACTS.map((f) => (
              <li key={f.title} className="border-b border-border py-3">
                <p className="text-[14.5px] leading-snug">
                  <TextLink href={f.href}>{f.title}</TextLink>
                </p>
                <p className="mt-1 text-[14px] leading-snug text-muted-foreground">{f.body}</p>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </Section>
  );
}

/** Agent ⇄ `sakura mcp serve` ⇄ compiler: requests travel right on the upper lane, answers back. */
function McpFigure() {
  const packets = [
    { cls: "mcp-out", x: 32, y: 36, d: 118, delay: 0, fill: "var(--sakura)" },
    { cls: "mcp-out", x: 290, y: 36, d: 118, delay: 0.7, fill: "var(--sakura)" },
    { cls: "mcp-out", x: 408, y: 44, d: -118, delay: 1.4, fill: "var(--ok)" },
    { cls: "mcp-out", x: 150, y: 44, d: -118, delay: 2.1, fill: "var(--ok)" },
  ];
  const label = { fontSize: 13, fontWeight: 600, fill: "var(--foreground)" } as const;
  const sub = { fontSize: 11, fill: "var(--muted-foreground)" } as const;
  return (
    <Play>
      <svg
        viewBox="0 0 440 146"
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- an inline, animated SVG figure
        role="img"
        aria-label="Your agent, Claude Code, Codex or Cursor, calls sakura mcp serve over stdio; the server runs the compiler and returns diagnostics, fixes and test results. Its ten tools: compiler_info, check, symbol_query, hole_context, docs_query, explain_diagnostic, format, check_patch, apply_patch and test."
        className="w-full"
      >
        <line x1="32" y1="40" x2="150" y2="40" stroke="var(--border-strong)" />
        <line x1="290" y1="40" x2="408" y2="40" stroke="var(--border-strong)" />
        {packets.map((p, i) => (
          <circle
            key={i}
            className={p.cls}
            cx={p.x}
            cy={p.y}
            r="3.5"
            fill={p.fill}
            style={{ "--mcp-d": `${p.d}px`, "--mcp-delay": `${p.delay}s` } as CSSProperties}
          />
        ))}
        <circle cx="24" cy="40" r="8" fill="var(--card)" stroke="var(--border-strong)" />
        <circle cx="416" cy="40" r="8" fill="var(--card)" stroke="var(--border-strong)" />
        <rect
          x="150"
          y="22"
          width="140"
          height="36"
          rx="18"
          fill="var(--sakura-wash)"
          stroke="var(--sakura)"
        />
        <text
          x="220"
          y="44.5"
          textAnchor="middle"
          fontSize="12"
          className="font-mono"
          fill="var(--foreground)"
        >
          sakura mcp serve
        </text>
        <text x="0" y="78" {...label}>
          your agent
        </text>
        <text x="0" y="94" {...sub}>
          Claude Code · Codex · Cursor
        </text>
        <text x="220" y="78" textAnchor="middle" {...sub}>
          10 tools over stdio
        </text>
        <text x="440" y="78" textAnchor="end" {...label}>
          compiler
        </text>
        <text x="440" y="94" textAnchor="end" {...sub}>
          the same answers as the CLI
        </text>
        <text
          x="220"
          y="122"
          textAnchor="middle"
          fontSize="10.5"
          className="font-mono"
          fill="var(--faint-foreground)"
        >
          {TOOLS.slice(0, 5).join(" · ")}
        </text>
        <text
          x="220"
          y="139"
          textAnchor="middle"
          fontSize="10.5"
          className="font-mono"
          fill="var(--faint-foreground)"
        >
          {TOOLS.slice(5).join(" · ")}
        </text>
      </svg>
    </Play>
  );
}
