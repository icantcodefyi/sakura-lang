import Link from "next/link";
import type { ReactNode } from "react";
import { ArrowUpRight } from "lucide-react";
import { Seal } from "@/components/seal";
import { site } from "@/lib/site";
import { Code } from "@/components/code";
import { llmsTxt } from "@/lib/source";
import { Tabs } from "./tabs";

const MCP = `// → tools/call (planned for M3; sakura check --json emits the same record today)
{ "name": "hole_context", "arguments": { "snapshot": "sha256:41c2…", "hole_id": "h:9b07…" } }

// ← result: the hole record of design §12.2
{
  "schema_version": "1.0",
  "kind": "hole",
  "id": "h:9b07…",
  "name": "receipt",
  "snapshot": "sha256:41c2…",
  "span": { "uri": "file:///app/src/cart.sak", "start_byte": 142, "end_byte": 150, "role": "primary" },
  "expected_type": { "render": "Receipt" },
  "allowed_effects": { "render": "{Payments, Fail<CheckoutError>}" },
  "locals": [
    { "name": "cart", "type": { "render": "Cart" } },
    { "name": "card", "type": { "render": "Card" } },
    { "name": "total", "type": { "render": "Int" } }
  ],
  "candidates": [{ "expr": "Payments.charge(card, total)", "fit": "function" }],
  "contracts_in_scope": []
}`;

const DIAGNOSTIC = `{
  "schema_version": "1.0",
  "kind": "diagnostic",
  "id": "d:7f3a",
  "code": "V3001",
  "severity": "error",
  "phase": "effect",
  "message": "function \`signup\` performs \`Http\`, which its signature does not allow",
  "snapshot": "sha256:9c1e…",
  "symbol_id": "app::users::signup",
  "spans": [{
    "uri": "file:///app/src/users.sak", "content_hash": "sha256:52b0…",
    "start_byte": 431, "end_byte": 461, "role": "cause",
    "label": "\`Http.post_json\` introduces \`Http\` here"
  }],
  "related_ids": [],
  "expected": { "render": "{Clock, Db, Random, Fail<SignupError>}", "row": { /* … */ } },
  "actual": { "render": "{Clock, Db, Http, Random, Fail<SignupError>}", "row": { /* … */ } },
  "obligation": null,
  "counterexample": null,
  "explanation_uri": "sakura://explain/V3001",
  "fixes": [
    { "id": "f:1", "title": "Add \`Http\` to the signature",
      "applicability": "suggested", "impact": "interface_change", "edits": [/* … */] },
    { "id": "f:2", "title": "Handle the effect locally with \`with\`",
      "applicability": "incomplete", "impact": "local", "edits": [/* … */] }
  ]
}`;

const HOLE = `fn checkout(cart: Cart, card: Card) -> Receipt ! {Payments, Fail<CheckoutError>} {
  let total = List.fold(cart.items, 0, fn(sum, i) { sum + i.price })
  ?receipt
}`;

const HOLE_REPLY = `hole ?receipt: expected Receipt, effects {Payments, Fail<CheckoutError>};
  locals [cart: Cart, card: Card, total: Int];
  candidates [Payments.charge(card, total)]`;

const PATHS = [
  { href: "/llms.txt", path: "/llms.txt", what: "Index of every docs page, with summaries." },
  { href: "/llms-full.txt", path: "/llms-full.txt", what: "All of the docs in one file." },
  { href: "/docs/tour/effects.md", path: "/docs/<page>.md", what: "Any page as plain Markdown." },
  {
    href: `${site.github}/blob/master/docs/llms.txt`,
    path: "docs/llms.txt",
    what: "The whole core language, in one file.",
  },
];

export async function AgentView() {
  const index = llmsTxt().split("\n").slice(0, 12).join("\n");
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
                label: "llms.txt",
                panel: (
                  <div>
                    <Code code={index} lang="md" title={<span>GET /llms.txt</span>} wrap />
                    <p className="mt-3 text-[13px] text-muted-foreground">
                      Live from this site.{" "}
                      {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
                      <a
                        href="/llms.txt"
                        className="font-medium text-foreground underline decoration-sakura underline-offset-4"
                      >
                        Open llms.txt
                      </a>{" "}
                      or{" "}
                      {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
                      <a
                        href="/llms-full.txt"
                        className="font-medium text-foreground underline decoration-sakura underline-offset-4"
                      >
                        llms-full.txt
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
                  <Code code={DIAGNOSTIC} lang="json" title={<span>sakura check --json</span>} />
                ),
              },
              {
                label: "Holes",
                panel: (
                  <div className="grid gap-3">
                    <Code
                      code={HOLE}
                      title={<span>src/cart.sak</span>}
                      numbers
                      mark={{ 2: "seal" }}
                    />
                    <Code code={HOLE_REPLY} lang="text" title={<span>sakura check</span>} />
                  </div>
                ),
              },
            ]}
          />
          <p className="mt-3 text-[12.5px] text-faint">
            Diagnostic and hole records are what{" "}
            <code className="font-mono">sakura check --json</code> emits today; the MCP server is
            planned for M3.
          </p>
        </div>
      </div>
      <div className="mx-auto max-w-6xl px-5 pt-16 pb-24 sm:px-8 md:pb-32">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <h3 className="font-display text-[1.25rem] font-semibold tracking-[-0.015em]">
            Start here if you are an agent
          </h3>
          <Link
            href="/docs/agents"
            className="text-[14.5px] font-medium underline decoration-sakura underline-offset-4"
          >
            For agents docs
          </Link>
        </div>
        <ul className="mt-5 grid border-t border-foreground/80 sm:grid-cols-2 lg:grid-cols-4">
          {PATHS.map((p) => (
            <li
              key={p.path}
              className="border-b border-border sm:odd:border-r lg:border-r lg:last:border-r-0"
            >
              {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text routes, not pages */}
              <a
                href={p.href}
                className="group block h-full px-1 py-5 transition-colors duration-150 hover:bg-muted sm:px-5"
              >
                <code className="font-mono text-[14px] text-foreground">
                  {p.path}
                  <ArrowUpRight className="ml-1 inline size-3.5 align-[-1px] text-faint transition-[transform,color] duration-200 group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-sakura-text" />
                </code>
                <span className="mt-1.5 block text-[14px] leading-snug text-muted-foreground">
                  {p.what}
                </span>
              </a>
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
    cmd: "apply fixes",
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
      <code className="block font-mono text-[12.5px] font-medium whitespace-nowrap text-foreground">
        {cmd}
        <span className="ml-2 font-sans text-[11px] font-normal text-faint">{who}</span>
      </code>
      <p className="mt-1 text-[12.5px] leading-snug text-muted-foreground">{what}</p>
    </>
  );
}
