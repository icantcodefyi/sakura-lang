import Link from "next/link";
import { ArrowUpRight } from "lucide-react";

/** Memory and toolchain, side by side: the parts underneath the promises. */
export function Pillars() {
  return (
    <section id="pillars" className="border-b border-line">
      <div className="mx-auto grid max-w-6xl px-5 py-24 sm:px-8 md:py-28 lg:grid-cols-2">
        <div className="pb-14 lg:pr-14 lg:pb-0">
          <Heading href="/docs/tour/memory">Values, without a borrow checker.</Heading>
          <p className="mt-4 max-w-[30rem] text-[16px] leading-relaxed text-pretty text-fg-muted">
            Every value is independent: no aliasing, no lifetimes, no cycles. Native code (M5)
            counts references at compile time and updates a uniquely owned value in place. The
            JavaScript backend uses the host GC; the semantics are the same.
          </p>
          <div className="mt-8 max-w-[22rem]">
            <ReuseDiagram />
          </div>
        </div>
        <div className="border-t border-line pt-14 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-14">
          <Heading href="/docs/stdlib">One binary for the whole loop.</Heading>
          <p className="mt-4 max-w-[30rem] text-[16px] leading-relaxed text-pretty text-fg-muted">
            Checker, runner, formatter and test runner already share one compiler library; packages,
            LSP and MCP join them in M3, with a standard library for HTTP, SQLite, JSON schemas,
            CLIs and typed LLM calls.
          </p>
          <div className="mt-8">
            <Toolchain />
          </div>
        </div>
      </div>
    </section>
  );
}

function Heading({ href, children }: { href: string; children: string }) {
  return (
    <h2 className="font-display text-[clamp(1.5rem,2.4vw,1.9rem)] leading-tight font-semibold tracking-[-0.025em]">
      <Link href={href} className="group rounded-sm">
        {children.split(" ").slice(0, -1).join(" ")}{" "}
        <span className="whitespace-nowrap">
          {children.split(" ").at(-1)}
          <ArrowUpRight className="ml-1 inline-block size-5 align-[-2px] text-fg-faint transition-[transform,color] duration-200 group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-seal-text" />
        </span>
      </Link>
    </h2>
  );
}

/** A uniquely-owned list is extended in place instead of copied. */
function ReuseDiagram() {
  const cells = ["1", "2", "3"];
  return (
    <svg viewBox="0 0 260 136" className="mx-auto w-full max-w-[18rem]" role="presentation">
      <text x="0" y="16" className="fill-fg-faint font-mono text-[10px]">
        xs = List.push(xs, 4)
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

// The first six exist in the compiler today; the rest are planned for M3.
const COMMANDS = [
  ["check", "types, effects, exhaustiveness, holes", ""],
  ["run", "build to JavaScript and run main", ""],
  ["test", "deterministic handlers, properties, snapshots", ""],
  ["fmt", "one canonical format", ""],
  ["explain", "any code, with a wrong and a fixed program", ""],
  ["prompt", "the language guide for agents", ""],
  ["fix", "fixes gated by impact class", "planned"],
  ["mcp serve", "the compiler as MCP tools", "planned"],
] as const;

function Toolchain() {
  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 border-t border-line font-mono text-[13px]">
      {COMMANDS.map(([cmd, what, planned]) => (
        <div key={cmd} className="col-span-2 grid grid-cols-subgrid border-b border-line py-2">
          <dt className="whitespace-nowrap">
            <span className="text-fg-faint">sakura </span>
            <span className={planned ? "text-fg-muted" : "text-seal-text"}>{cmd}</span>
          </dt>
          <dd className="font-sans text-[13.5px] text-fg-muted">
            {what}
            {planned ? (
              <span className="ml-2 rounded-full border border-dashed border-line-strong px-1.5 py-px font-mono text-[11px] text-fg-muted">
                {planned}
              </span>
            ) : null}
          </dd>
        </div>
      ))}
    </dl>
  );
}
