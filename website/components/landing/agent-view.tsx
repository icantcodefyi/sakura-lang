import Link from "next/link";
import type { ReactNode } from "react";
import { ArrowUpRight } from "lucide-react";
import { Seal } from "@/components/seal";
import { Code } from "@/components/code";
import { sample } from "./samples";
import { Tabs } from "./tabs";

// Every output below is the compiler's, recorded from main on the checked samples in
// ./samples (long values trimmed with …). Re-record them when the record shapes change.

const SETUP = `$ sakura new cli app && cd app
$ sakura setup --client claude --apply
sakura setup --client claude: ~/app (writing)
  keep   AGENTS.md (exists)
  keep   CLAUDE.md (exists)
  create .claude/skills/sakura/SKILL.md
  create .claude/skills/sakura/assets/AGENTS.md
  create .claude/skills/sakura/assets/CLAUDE.md
  create .claude/skills/sakura/assets/mcp-claude.json
  create .claude/skills/sakura/assets/mcp-codex.toml
  create .claude/skills/sakura/assets/mcp-cursor.json
  create .claude/skills/sakura/assets/sakura.mdc
  create .claude/skills/sakura/references/diagnostics.md
  create .claude/skills/sakura/references/language.md
  create .claude/skills/sakura/references/recipes.md
  create .claude/skills/sakura/references/repairs.md
  create .claude/skills/sakura/references/stdlib.md
  create .mcp.json`;

const MCP = `// → tools/call
{ "name": "hole_context", "arguments": { "snapshot": "sha256:fb278f9e…", "hole_id": "h:8e8d4a0debc0ca2c" } }

// ← result
{
  "hole": {
    "kind": "hole",
    "name": "receipt",
    "expected_type": { "render": "Receipt" },
    "allowed_effects": { "render": "{Payments, Fail<Declined | Expired>}", "row": { … } },
    "locals": [
      { "name": "cart", "type": { "render": "Cart" } },
      { "name": "card", "type": { "render": "Card" } },
      { "name": "total", "type": { "render": "Int" } }
    ],
    "candidates": [{ "expr": "charge(?card, ?amount)", "fit": "function" }],
    "contracts_in_scope": []
  },
  "enclosing": {
    "id": "app::main::checkout",
    "signature": "fn checkout(cart: Cart, card: Card) -> Receipt ! {Payments, Fail<CheckoutError>}", …
  },
  "related": [{ "id": "app::main::charge", … }]
}`;

const DIAGNOSTIC = `{
  "schema_version": "1.0",
  "kind": "diagnostic",
  "id": "d:55fbcebe8f6a7c64",
  "code": "V3001",
  "severity": "error",
  "phase": "effect",
  "message": "function \`signup\` performs \`Http\` and \`Fail<HttpError | RateLimited>\`, which its signature does not allow (allowed: \`{Clock, Db, Random, Fail<Banned | EmailTaken>}\`)",
  "symbol_id": "app::users::signup",
  "spans": [
    { "uri": "file:///app/src/users.sak", "start_byte": 87, "end_byte": 93, "role": "primary" },
    { "uri": "file:///app/src/users.sak", "start_byte": 379, "end_byte": 409, "role": "cause",
      "label": "\`post_json\` performs \`Http\` here" }, …
  ],
  "expected": { "render": "{Clock, Db, Random, Fail<Banned | EmailTaken>}", "row": { … } },
  "actual": { "render": "{Clock, Db, Http, Random, Fail<EmailTaken | HttpError | RateLimited>}", "row": { … } },
  "explanation_uri": "sakura://explain/V3001",
  "fixes": [
    { "id": "f:55fbcebe8f6a7c64.0",
      "title": "Add \`Http\` and \`Fail<HttpError | RateLimited>\` to the signature",
      "applicability": "suggested", "impact": "interface_change", "edits": [ … ] },
    { "id": "f:55fbcebe8f6a7c64.1",
      "title": "Handle the effect locally with \`with Http.scripted(…)\`",
      "applicability": "incomplete", "impact": "local", "edits": [ … ] }
  ]
}`;

const HOLE = sample("checkout");

const HOLE_REPLY =
  "hole ?receipt: expected Receipt, effects {Payments, Fail<Declined | Expired>}; locals [cart: Cart, card: Card, total: Int]; candidates [charge(?card, ?amount)]";

// What ships in v0.1.0-alpha.1 for an agent, each with the page that documents it.
const SHIPPED: { name: ReactNode; what: string; href: string }[] = [
  {
    name: "Skill",
    what: "sakura setup writes SKILL.md, its references and the MCP entry for Claude, Codex or Cursor.",
    href: "/docs/agents",
  },
  {
    name: "MCP",
    what: "check, check_patch, apply_patch, hole_context, test and five more tools over stdio.",
    href: "/docs/agents/mcp",
  },
  {
    name: "LSP",
    what: "Diagnostics, hover, completion, formatting and quick fixes that state their impact.",
    href: "/docs/editors",
  },
  {
    name: <code className="font-mono">--json</code>,
    what: "Versioned diagnostic records; every fix says how it applies and how far it reaches.",
    href: "/docs/agents/diagnostics",
  },
  {
    name: "Holes",
    what: "A ?name passes check and reports its type, effects, locals and candidates.",
    href: "/docs/agents/typed-holes",
  },
  {
    name: "Grants",
    what: "Effects live in signatures; [permissions] in sakura.toml caps what the host hands out.",
    href: "/docs/tour/capabilities",
  },
  {
    name: <code className="font-mono">verify</code>,
    what: "Z3 proves contracts and arithmetic, cvc5 cross-checks, and builds drop proved checks.",
    href: "/docs/agents/verify",
  },
  {
    name: "Deterministic",
    what: "The same input gives the same bytes; tests run under fixed seeds and test handlers.",
    href: "/docs/tour/testing",
  },
];

export function AgentView() {
  return (
    <section id="agents" className="border-b border-border bg-muted/50">
      <div className="mx-auto grid max-w-6xl grid-cols-1 gap-12 px-5 pt-24 sm:px-8 md:pt-32 lg:grid-cols-[0.8fr_1.2fr] lg:gap-16">
        <div>
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            How an agent works with Sakura Lang.
          </h2>
          <p className="mt-5 max-w-[30rem] text-[17px] leading-relaxed text-pretty text-muted-foreground">
            The compiler is the agent&apos;s other half. Every answer it gives is data a model can
            act on, and every change it would make says how far it reaches.
          </p>
          <AgentLoop />
        </div>
        <div className="min-w-0">
          <Tabs
            items={[
              {
                label: "Skill",
                panel: (
                  <div>
                    <Code code={SETUP} lang="text" title={<span>terminal</span>} />
                    <p className="mt-3 text-[13px] text-muted-foreground">
                      Or fetch it directly:{" "}
                      {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- static file, not a page */}
                      <a
                        href="/skills/sakura/SKILL.md"
                        className="font-medium text-foreground underline decoration-sakura underline-offset-4"
                      >
                        SKILL.md
                      </a>{" "}
                      or the{" "}
                      {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- static file, not a page */}
                      <a
                        href="/skills/sakura-skill.zip"
                        className="font-medium text-foreground underline decoration-sakura underline-offset-4"
                      >
                        checksummed ZIP
                      </a>
                      .
                    </p>
                  </div>
                ),
              },
              {
                label: "MCP",
                panel: <Code code={MCP} lang="jsonc" title={<span>sakura mcp serve</span>} />,
              },
              {
                label: "Diagnostics",
                short: "Errors",
                panel: (
                  <Code code={DIAGNOSTIC} lang="jsonc" title={<span>sakura check --json</span>} />
                ),
              },
              {
                label: "Holes",
                panel: (
                  <div className="grid gap-3">
                    <Code
                      code={HOLE}
                      title={<span>src/main.sak</span>}
                      numbers
                      mark={{ 6: "seal" }}
                    />
                    <Code code={HOLE_REPLY} lang="text" title={<span>sakura check</span>} wrap />
                  </div>
                ),
              },
            ]}
          />
          <p className="mt-3 text-[12.5px] text-faint">
            Recorded from the current compiler on the hero&apos;s program and the one in the Holes
            tab; long values are trimmed.
          </p>
        </div>
      </div>
      <div className="mx-auto max-w-6xl px-5 pt-16 pb-24 sm:px-8 md:pb-32">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <h3 className="font-display text-[1.25rem] font-semibold tracking-[-0.015em]">
            In the alpha.1 binary
          </h3>
          <p className="text-[14.5px] text-muted-foreground">
            Agents can also{" "}
            {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
            <a
              href="/llms.txt"
              className="font-medium text-foreground underline decoration-sakura underline-offset-4"
            >
              GET /llms.txt
            </a>{" "}
            or{" "}
            <Link
              href="/docs/agents"
              className="font-medium text-foreground underline decoration-sakura underline-offset-4"
            >
              read the agent docs
            </Link>
            .
          </p>
        </div>
        <ul className="mt-5 grid border-t border-foreground/80 sm:grid-cols-2 lg:grid-cols-4">
          {SHIPPED.map((p) => (
            <li key={p.href} className="border-b border-border">
              <Link
                href={p.href}
                className="group block h-full py-5 transition-colors duration-150 hover:bg-muted sm:px-5"
              >
                <span className="font-display text-[1.05rem] font-semibold tracking-[-0.01em] text-foreground">
                  {p.name}
                  <ArrowUpRight className="ml-1 inline size-3.5 align-[-1px] text-faint transition-[transform,color] duration-200 group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-sakura-text" />
                </span>
                <span className="mt-1.5 block text-[14px] leading-snug text-muted-foreground">
                  {p.what}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

const LOOP = [
  {
    cmd: "write",
    who: "agent",
    what: (
      <>
        drafts the body, leaves <code className="font-mono whitespace-nowrap">?holes</code> where it
        is unsure
      </>
    ),
  },
  {
    cmd: "sakura check --json",
    who: "compiler",
    what: "diagnostics and hole records as JSON Lines",
  },
  {
    cmd: "sakura fix",
    who: "agent",
    what: "local fixes go in; signature changes go to a person",
  },
  {
    cmd: "sakura test",
    who: "compiler",
    what: "deterministic handlers, fixed seed, shrunk failures",
  },
];

/** The agent ⇄ compiler loop: four stations around one orbit (a list on phones). */
function AgentLoop() {
  return (
    <div className="mt-10">
      <ol className="relative grid gap-5 pl-7 sm:hidden">
        <span aria-hidden className="absolute top-2 bottom-2 left-[5px] w-px bg-strong" />
        {LOOP.map((s) => (
          <li key={s.cmd} className="relative">
            <span
              aria-hidden
              className="absolute top-1.5 -left-7 size-[11px] rounded-full border-2 border-sakura bg-background"
            />
            <Station {...s} />
          </li>
        ))}
        <li className="text-[13px] text-faint">↺ until check and test are clean</li>
      </ol>
      <div className="relative hidden grid-cols-2 gap-x-14 gap-y-24 py-2 sm:grid">
        {/* the ring the loop runs on, with the seal it is working towards */}
        <div
          aria-hidden
          className="absolute top-1/2 left-1/2 size-44 -translate-x-1/2 -translate-y-1/2"
        >
          <span className="absolute inset-0 rounded-full border border-dashed border-strong" />
          <span className="absolute inset-0 motion-safe:animate-[spin_9s_linear_infinite]">
            <span className="absolute -top-[5px] left-1/2 size-2.5 -translate-x-1/2 rounded-full bg-sakura shadow-[0_0_0_4px_var(--sakura-wash)]" />
          </span>
          <Seal className="absolute top-1/2 left-1/2 size-16 -translate-x-1/2 -translate-y-1/2 -rotate-12" />
        </div>
        {[0, 1, 3, 2].map((i) => (
          <div
            key={LOOP[i].cmd}
            className="relative rounded-xl border border-border bg-card px-3.5 py-3 shadow-[0_8px_24px_-16px_hsl(var(--shadow-color)/0.4)]"
          >
            <span className="mb-1.5 block font-mono text-[11px] text-faint tabular-nums">
              {String(i + 1).padStart(2, "0")}
            </span>
            <Station {...LOOP[i]} />
          </div>
        ))}
      </div>
    </div>
  );
}

function Station({ cmd, who, what }: { cmd: string; who: string; what: ReactNode }) {
  return (
    <>
      <p className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
        <code className="font-mono text-[12.5px] font-medium whitespace-nowrap text-foreground">
          {cmd}
        </code>
        <span className="text-[11px] text-faint">{who}</span>
      </p>
      <p className="mt-1 text-[12.5px] leading-snug text-muted-foreground">{what}</p>
    </>
  );
}
