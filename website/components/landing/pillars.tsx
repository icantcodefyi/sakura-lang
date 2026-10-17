import Link from "next/link";
import type { ReactNode } from "react";
import { ArrowUpRight } from "lucide-react";
import { TokenSpans } from "@/components/token-spans";
import { tokenize } from "@/lib/vow/highlight";

async function Snippet({
  code,
  lang = "vow",
  marks,
}: {
  code: string;
  lang?: string;
  marks?: Record<number, string>;
}) {
  const lines = await tokenize(code, lang);
  return (
    <pre className="overflow-x-auto font-mono text-[11.5px] leading-[1.75] sm:text-[12px]">
      <code className="grid min-w-max">
        {lines.map((l, i) => (
          <span key={i} className="flex items-center justify-between gap-6">
            <span className="whitespace-pre">
              <TokenSpans line={l} />
              {l.length === 0 ? " " : null}
            </span>
            {marks?.[i] ? (
              <span className="font-sans text-[11px] whitespace-nowrap">{marks[i]}</span>
            ) : null}
          </span>
        ))}
      </code>
    </pre>
  );
}

function Cell({
  title,
  href,
  children,
  visual,
  className,
  visualClassName,
  wide,
}: {
  title: string;
  href: string;
  children: ReactNode;
  visual: ReactNode;
  className?: string;
  visualClassName?: string;
  wide?: boolean;
}) {
  return (
    <li
      className={`group flex flex-col overflow-hidden rounded-2xl border border-line bg-surface ${wide ? "lg:flex-row-reverse" : ""} ${className ?? ""}`}
    >
      <div
        aria-hidden
        className={`flex flex-1 flex-col justify-center border-b border-line p-5 sm:p-6 ${wide ? "lg:w-[60%] lg:flex-none lg:border-b-0 lg:border-l" : ""} ${visualClassName ?? "bg-bg-sunk"}`}
      >
        {visual}
      </div>
      <div
        className={`p-5 sm:p-6 ${wide ? "lg:flex lg:flex-1 lg:flex-col lg:justify-center" : ""}`}
      >
        <h3 className="font-display text-[1.3rem] leading-tight font-semibold tracking-[-0.02em]">
          <Link
            href={href}
            className="rounded-sm after:absolute after:inset-0 after:rounded-2xl focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:outline-seal"
          >
            {title.split(" ").slice(0, -1).join(" ")}{" "}
            <span className="whitespace-nowrap">
              {title.split(" ").at(-1)}
              <ArrowUpRight className="ml-1 inline-block size-4 align-[-1px] text-fg-faint transition-[transform,color] duration-200 group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-seal-text" />
            </span>
          </Link>
        </h3>
        <p className="mt-2 max-w-[46ch] text-[15px] leading-relaxed text-pretty text-fg-muted">
          {children}
        </p>
      </div>
    </li>
  );
}

export async function Pillars() {
  return (
    <section id="pillars" className="border-b border-line bg-bg-sunk/50">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="max-w-2xl">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Everything a function does, written where you can check it.
          </h2>
          <p className="mt-5 max-w-[34rem] text-[17px] leading-relaxed text-pretty text-fg-muted">
            Six design commitments, each aimed at code that is written by a model and trusted by a
            person.
          </p>
        </div>

        <ul className="mt-14 grid gap-4 [&>li]:relative lg:grid-cols-6">
          <Cell
            className="lg:col-span-4"
            title="Effects you can see"
            href="/docs/tour/effects"
            visual={
              <div className="grid gap-4">
                <Snippet
                  code={`fn signup(email: Email, pw: Password) -> User ! {Db, Clock, Fail<SignupError>}`}
                />
                <div className="grid gap-2 sm:grid-cols-2">
                  <div className="rounded-lg border border-line bg-code px-3 py-2.5">
                    <p className="mb-1 font-sans text-[11px] font-medium text-fg-faint">
                      In production
                    </p>
                    <Snippet
                      code={`with Db.postgres(url), Clock.system() {\n  signup(email, pw)\n}`}
                    />
                  </div>
                  <div className="rounded-lg border border-line bg-code px-3 py-2.5">
                    <p className="mb-1 font-sans text-[11px] font-medium text-fg-faint">In tests</p>
                    <Snippet code={`with Db.memory(), Clock.fixed(t0) {\n  signup(email, pw)\n}`} />
                  </div>
                </div>
              </div>
            }
          >
            Database, clock, network and failure are typed effects in the signature. No exceptions,
            no null, no hidden I/O. Handlers decide what an effect means, so tests swap Postgres for
            memory without mocks.
          </Cell>

          <Cell
            className="lg:col-span-2"
            title="Fast, without a borrow checker"
            href="/docs/tour/memory"
            visual={<ReuseDiagram />}
          >
            Values, not references. Reference counts are inserted at compile time and unique values
            are updated in place. No GC pauses, no lifetimes. Native, WASM and JS targets.
          </Cell>

          <Cell
            className="lg:col-span-2"
            title="Verified by default"
            href="/docs/tour/contracts"
            visual={
              <Snippet
                code={`type Age = Int where 0..=150\n\nfn withdraw(a: Account, amt: Cents)\n  -> Account ! {Fail<Overdrawn>}\n  requires amt > 0\n  ensures result.balance < a.balance`}
              />
            }
          >
            Refinement types and contracts are part of the language. What the checker cannot prove
            becomes generated property tests, and simulation tests replay every failure by seed.
          </Cell>

          <Cell
            className="lg:col-span-2"
            title="Secure by construction"
            href="/docs/tour/capabilities"
            visual={
              <div className="grid gap-3">
                <Snippet
                  lang="toml"
                  code={`[dependencies]\nmarkdown = { version = "2.2" }\ns3 = { version = "0.4", grants = ["Net"] }`}
                />
                <p className="rounded-lg border border-fail/35 bg-fail-wash px-3 py-2 font-mono text-[11.5px] leading-relaxed text-fg">
                  <span className="font-semibold text-fail">E0501</span> markdown 2.2 asks for{" "}
                  <span className="text-seal-text">Net</span>, which was never granted
                </p>
              </div>
            }
          >
            A dependency can only touch the network, files or environment if you hand it that
            capability. <code className="font-mono text-[13.5px]">Untrusted&lt;T&gt;</code> blocks
            injection, <code className="font-mono text-[13.5px]">Secret&lt;T&gt;</code> cannot be
            logged.
          </Cell>

          <Cell
            className="lg:col-span-2"
            title="Built for agents"
            href="/docs/agents"
            visual={
              <div className="grid gap-3">
                <Snippet code={`fn total(items: List<Item>) -> Cents {\n  todo()\n}`} />
                <div className="rounded-lg border border-line bg-code px-3 py-2 font-mono text-[11.5px] leading-relaxed">
                  <p>
                    <span className="text-seal-text">hole</span>{" "}
                    <span className="text-fg-muted">expects</span> Cents
                  </p>
                  <p className="text-fg-muted">
                    fits: <span className="text-fg">items.map(i =&gt; i.price).sum()</span>
                  </p>
                </div>
              </div>
            }
          >
            A grammar small enough for llms.txt, one way to do each thing, typed holes, JSON
            diagnostics with fixes, a compiler MCP server, and content-addressed code so parallel
            agents do not collide.
          </Cell>

          <Cell
            className="lg:col-span-6"
            wide
            title="Batteries included"
            href="/docs/stdlib"
            visualClassName="bg-code"
            visual={<Toolchain />}
          >
            One binary for the whole loop. HTTP, typed SQL checked against your schema, full-stack
            web with typed RPC, durable workflows, and LLM calls as typed effects with budgets.
          </Cell>
        </ul>
        <p className="mt-4 text-[12.5px] text-fg-faint">
          Error codes and messages are illustrative; the compiler is not built yet.
        </p>
      </div>
    </section>
  );
}

/** A uniquely-owned list is extended in place instead of copied. */
function ReuseDiagram() {
  const cells = ["1", "2", "3"];
  return (
    <svg viewBox="0 0 260 136" className="mx-auto w-full max-w-[18rem]" role="presentation">
      <text x="0" y="16" className="fill-fg-faint font-mono text-[10px]">
        let ys = xs.push(4)
      </text>
      {/* rc badge */}
      <g transform="translate(0 38)">
        {cells.map((c, i) => (
          <g key={c} transform={`translate(${i * 44} 0)`}>
            <rect
              width="40"
              height="40"
              rx="7"
              className="fill-surface stroke-line-strong"
              strokeWidth="1"
            />
            <text x="20" y="25" textAnchor="middle" className="fill-fg font-mono text-[13px]">
              {c}
            </text>
          </g>
        ))}
        <g transform="translate(132 0)">
          <rect
            width="40"
            height="40"
            rx="7"
            className="fill-seal-wash stroke-seal"
            strokeWidth="1.5"
            strokeDasharray="0"
          />
          <text
            x="20"
            y="25"
            textAnchor="middle"
            className="fill-seal-text font-mono text-[13px] font-semibold"
          >
            4
          </text>
        </g>
        <rect x="186" y="9" width="62" height="22" rx="11" className="fill-ok-wash" />
        <text x="217" y="24" textAnchor="middle" className="fill-ok font-mono text-[10.5px]">
          rc = 1
        </text>
      </g>
      <text x="0" y="112" className="fill-fg-muted font-sans text-[11px]">
        unique, so the buffer is
      </text>
      <text x="0" y="128" className="fill-fg font-sans text-[11px] font-semibold">
        reused in place: no copy
      </text>
    </svg>
  );
}

const COMMANDS = [
  ["new", "scaffold a project"],
  ["check", "types, effects, contracts"],
  ["test", "tests, properties, simulation"],
  ["run", "interpret instantly"],
  ["build", "native, wasm, js"],
  ["fmt", "one canonical format"],
  ["mcp", "compiler as MCP server"],
] as const;

function Toolchain() {
  return (
    <ul className="grid gap-x-6 gap-y-3 font-mono text-[12.5px] sm:grid-cols-2">
      {COMMANDS.map(([cmd, what]) => (
        <li key={cmd} className="flex items-baseline gap-2">
          <span className="text-fg-faint">$</span>
          <span>
            vow <span className="text-seal-text">{cmd}</span>
          </span>
          <span className="truncate font-sans text-[12.5px] text-fg-muted">{what}</span>
        </li>
      ))}
      <li className="flex items-baseline gap-2 text-fg-muted">
        <span className="text-fg-faint">#</span>
        <span className="font-sans text-[12.5px]">std: Http, Sql, Rpc, Workflow, Llm, Budget</span>
      </li>
    </ul>
  );
}
