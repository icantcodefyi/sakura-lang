import Link from "next/link";
import { Code } from "@/components/code";
import { llmsTxt } from "@/lib/source";
import { Tabs } from "./tabs";

const MCP = `// → tools/call
{ "name": "hole_context", "arguments": { "snapshot": "s:41c2", "hole_id": "h:receipt" } }

// ← result
{
  "expected_type": "Receipt",
  "allowed_effects": "{Payments, Fail<CheckoutError>}",
  "locals": [
    { "name": "cart", "type": "Cart" },
    { "name": "card", "type": "Card" },
    { "name": "total", "type": "Int" }
  ],
  "candidates": [{ "expr": "Payments.charge(card, total)", "fit": "result_unifies" }],
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
    "uri": "file:///app/src/users.vow", "content_hash": "sha256:52b0…",
    "start_byte": 431, "end_byte": 461, "role": "cause",
    "label": "\`Http.post_json\` introduces \`Http\` here"
  }],
  "related_ids": [],
  "expected": { "render": "{Clock, Db, Random, Fail<SignupError>}", "row": { /* … */ } },
  "actual": { "render": "{Clock, Db, Http, Random, Fail<SignupError>}", "row": { /* … */ } },
  "obligation": null,
  "counterexample": null,
  "explanation_uri": "vow://explain/V3001",
  "fixes": [
    { "id": "f:1", "title": "Add \`Http\` to the signature of \`signup\`",
      "applicability": "suggested", "impact": "interface_change", "edits": [/* … */] },
    { "id": "f:2", "title": "Handle \`Http\` locally",
      "applicability": "incomplete", "impact": "local", "edits": [] }
  ]
}`;

const HOLE = `fn checkout(cart: Cart, card: Card) -> Receipt ! {Payments, Fail<CheckoutError>} {
  let total = cart.items.map(fn(i) { i.price }).sum()
  ?receipt
}`;

const HOLE_REPLY = `hole ?receipt at src/cart.vow:3:3
  expected  Receipt
  effects   {Payments, Fail<CheckoutError>}
  locals    cart: Cart, card: Card, total: Int
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
                "check, symbol_query, hole_context, check_patch and test as tools.",
              ],
              [
                "/docs/agents/diagnostics",
                "Errors are data",
                "JSON Lines with byte spans, expected and actual, and fixes with an impact class.",
              ],
              [
                "/docs/agents/typed-holes",
                "Holes, not hallucinations",
                "?name reports the expected type, allowed effects, locals and what fits.",
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
                panel: <Code code={MCP} lang="jsonc" title={<span>vow mcp serve</span>} />,
              },
              {
                label: "Diagnostics",
                panel: <Code code={DIAGNOSTIC} lang="json" title={<span>vow check --json</span>} />,
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
            Shapes follow design §12; the compiler is under construction and not released.
          </p>
        </div>
      </div>
    </section>
  );
}
