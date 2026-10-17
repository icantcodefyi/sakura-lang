import Link from "next/link";
import { Code } from "@/components/code";
import { llmsTxt } from "@/lib/source";
import { Tabs } from "./tabs";

const MCP = `// → tools/call
{ "name": "hole", "arguments": { "file": "src/cart.vow", "line": 14 } }

// ← result
{
  "expected": "Cents",
  "effects_allowed": [],
  "in_scope": { "items": "List<Item>", "discount": "Percent" },
  "candidates": ["items.map(i => i.price).sum()", "Cents.zero"]
}`;

const DIAGNOSTIC = `{
  "code": "E0402",
  "severity": "error",
  "message": "effect \`Net\` is performed but not declared in the signature of \`signup\`",
  "span": { "file": "src/users.vow", "start": { "line": 12, "col": 3 }, "end": { "line": 12, "col": 28 } },
  "expected": "! {Db, Clock, Random, Fail<SignupError>}",
  "found": "! {Db, Clock, Random, Net, Fail<SignupError>}",
  "fixes": [
    { "title": "Declare \`Net\` in the signature", "safety": "changes-signature" },
    { "title": "Remove the call to \`Http.post\`", "safety": "changes-behaviour" }
  ],
  "docs": "/docs/tour/effects.md#undeclared-effects"
}`;

const HOLE = `fn checkout(cart: Cart, card: Card) -> Receipt ! {Payments, Fail<CheckoutError>} {
  let total = cart.items.map(i => i.price).sum()
  let receipt = todo()
  receipt
}`;

const HOLE_REPLY = `hole at src/cart.vow:3:17
  expected  Receipt
  effects   Payments, Fail<CheckoutError>
  in scope  cart: Cart, card: Card, total: Cents
  fits      Payments.charge(card, total)`;

export async function AgentView() {
  const index = llmsTxt().split("\n").slice(0, 18).join("\n");
  return (
    <section id="agents" className="border-b border-line">
      <div className="mx-auto grid max-w-6xl grid-cols-1 gap-12 px-5 py-24 sm:px-8 md:py-32 lg:grid-cols-[0.75fr_1.25fr] lg:gap-16">
        <div>
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            How an agent sees Vow.
          </h2>
          <p className="mt-5 max-w-[30rem] text-[17px] leading-relaxed text-pretty text-fg-muted">
            Every surface of the language is something a model can read, query and act on without
            guessing.
          </p>
          <ul className="mt-10 grid max-w-[30rem] gap-px overflow-hidden rounded-xl border border-line bg-line text-[15px]">
            {[
              [
                "/docs/agents/llms-txt",
                "The whole language fits in context",
                "llms.txt, llms-full.txt, and every page as Markdown.",
              ],
              [
                "/docs/agents/mcp",
                "The compiler is an MCP server",
                "Check, explain, find, test and fix as tool calls.",
              ],
              [
                "/docs/agents/diagnostics",
                "Errors are data",
                "JSON with spans, expected and found, and ranked fixes.",
              ],
              [
                "/docs/agents/typed-holes",
                "Holes, not hallucinations",
                "todo() returns the type, the effects and what fits.",
              ],
            ].map(([href, title, body]) => (
              <li key={href} className="bg-bg">
                <Link
                  href={href}
                  className="block px-5 py-4 transition-colors duration-150 hover:bg-bg-sunk"
                >
                  <span className="block font-medium">{title}</span>
                  <span className="mt-0.5 block text-[14px] text-fg-muted">{body}</span>
                </Link>
              </li>
            ))}
          </ul>
        </div>

        <div className="min-w-0">
          <Tabs
            items={[
              {
                label: "llms.txt",
                panel: (
                  <div>
                    <Code code={index} lang="md" title={<span>GET /llms.txt</span>} wrap />
                    <p className="mt-3 text-[13px] text-fg-muted">
                      Live from this site.{" "}
                      {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
                      <a
                        href="/llms.txt"
                        className="font-medium text-fg underline decoration-seal underline-offset-4"
                      >
                        Open llms.txt
                      </a>{" "}
                      or{" "}
                      {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
                      <a
                        href="/llms-full.txt"
                        className="font-medium text-fg underline decoration-seal underline-offset-4"
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
                panel: <Code code={MCP} lang="jsonc" title={<span>vow mcp</span>} />,
              },
              {
                label: "Diagnostics",
                panel: (
                  <Code
                    code={DIAGNOSTIC}
                    lang="json"
                    title={<span>vow check --format json</span>}
                  />
                ),
              },
              {
                label: "Typed holes",
                panel: (
                  <div className="grid gap-3">
                    <Code
                      code={HOLE}
                      title={<span>src/cart.vow</span>}
                      numbers
                      mark={{ 2: "seal" }}
                    />
                    <Code code={HOLE_REPLY} lang="text" title={<span>vow check</span>} />
                  </div>
                ),
              },
            ]}
          />
          <p className="mt-3 text-[12.5px] text-fg-faint">
            Tool output shapes follow the design docs; the compiler is not released.
          </p>
        </div>
      </div>
    </section>
  );
}
