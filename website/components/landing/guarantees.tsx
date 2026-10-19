import Link from "next/link";

// The four statuses of design §1.2, each with what establishes it and real examples.
const ROWS = [
  {
    status: "proved",
    tone: "text-ok border-ok/45 bg-ok-wash",
    by: "the checker on every build, or sakura verify",
    examples: [
      "every effect has a handler",
      "every Fail tag is caught or declared",
      "every match is exhaustive",
      "no null, no unchecked index",
    ],
  },
  {
    status: "runtime_checked",
    tone: "text-sakura-text border-sakura/55 bg-sakura-wash",
    by: "a check at the boundary; a violation is a defect",
    examples: ["requires ordered: lo <= hi", "where it >= 0 && it <= 100", "Int overflow"],
  },
  {
    status: "tested",
    tone: "text-foreground border-strong bg-muted",
    by: "sakura test, on the inputs that ran",
    examples: ['property "normalize is idempotent"', "sakura test --contracts", "snapshots"],
  },
  {
    status: "trusted",
    tone: "text-muted-foreground border-dashed border-strong",
    by: "assumed, and listed in the build manifest",
    examples: ["@external bindings", '@trust("reason")', "host handlers"],
  },
];

export function Guarantees() {
  return (
    <section className="border-b border-border">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,24rem)] md:items-end md:gap-16">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Every claim says how it is known.
          </h2>
          <p className="text-[17px] leading-relaxed text-pretty text-muted-foreground">
            Sakura Lang never calls a program verified because it compiled. Each obligation gets
            exactly one status, and the tools report it.
          </p>
        </div>

        <ol className="mt-12 grid border-t border-foreground/80">
          {ROWS.map((r, i) => (
            <li
              key={r.status}
              className="grid gap-x-8 gap-y-3 border-b border-border py-6 md:grid-cols-[2.5rem_13rem_minmax(0,1fr)] lg:grid-cols-[2.5rem_13rem_minmax(0,16rem)_minmax(0,1fr)]"
            >
              <span className="hidden font-mono text-[12px] text-faint tabular-nums md:block md:pt-1.5">
                {String(i + 1).padStart(2, "0")}
              </span>
              <span>
                <span
                  className={`inline-block rounded-md border px-2.5 py-1 font-mono text-[13px] font-medium ${r.tone}`}
                >
                  {r.status}
                </span>
              </span>
              <p className="text-[15px] leading-snug text-foreground md:pt-1">{r.by}</p>
              <ul className="flex flex-wrap gap-x-2 gap-y-2 md:col-start-3 lg:col-start-auto">
                {r.examples.map((e) => (
                  <li
                    key={e}
                    className="rounded-full border border-border bg-card px-2.5 py-0.5 font-mono text-[12px] text-muted-foreground"
                  >
                    {e}
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ol>
        <p className="mt-6 max-w-[44rem] text-[14.5px] leading-relaxed text-muted-foreground">
          Every contract starts <code className="font-mono text-foreground">runtime_checked</code>,
          in every build including <code className="font-mono text-foreground">--release</code>.{" "}
          <code className="font-mono text-foreground">sakura verify</code> asks Z3, cross-checked by
          cvc5 when it is installed, to prove each obligation; a proof moves that obligation to{" "}
          <code className="font-mono text-foreground">proved</code> and the next build drops its
          check where it is eligible, while a failure comes back as a counterexample.{" "}
          <Link
            href="/docs/agents/verify"
            className="font-medium text-foreground underline decoration-sakura underline-offset-4"
          >
            Verify contracts
          </Link>
        </p>
      </div>
    </section>
  );
}
