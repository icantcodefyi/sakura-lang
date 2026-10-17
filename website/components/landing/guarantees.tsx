import Link from "next/link";

// The four statuses of design §1.2, each with what establishes it and real examples.
const ROWS = [
  {
    status: "proved",
    tone: "text-ok border-ok/45 bg-ok-wash",
    by: "the type and effect checker, on every build",
    examples: [
      "every effect has a handler",
      "every Fail tag is caught or declared",
      "every match is exhaustive",
      "no null, no unchecked index",
    ],
  },
  {
    status: "runtime_checked",
    tone: "text-seal-text border-seal/55 bg-seal-wash",
    by: "a check at the boundary; a violation is a defect",
    examples: ["requires ordered: lo <= hi", "where it >= 0 && it <= 100", "Int overflow"],
  },
  {
    status: "tested",
    tone: "text-fg border-line-strong bg-bg-sunk",
    by: "sakura test, on the inputs that ran",
    examples: ['property "normalize is idempotent"', "sakura test --contracts", "snapshots"],
  },
  {
    status: "trusted",
    tone: "text-fg-muted border-dashed border-line-strong",
    by: "assumed, and listed in the build manifest",
    examples: ["@external bindings", '@trust("reason")', "host handlers"],
  },
];

export function Guarantees() {
  return (
    <section className="border-b border-line">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,24rem)] md:items-end md:gap-16">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Every claim says how it is known.
          </h2>
          <p className="text-[17px] leading-relaxed text-pretty text-fg-muted">
            Sakura Lang never calls a program verified because it compiled. Each obligation gets
            exactly one status, and the tools report it.
          </p>
        </div>

        <ol className="mt-12 grid border-t border-fg/80">
          {ROWS.map((r, i) => (
            <li
              key={r.status}
              className="grid gap-x-8 gap-y-3 border-b border-line py-6 md:grid-cols-[2.5rem_13rem_minmax(0,1fr)] lg:grid-cols-[2.5rem_13rem_minmax(0,16rem)_minmax(0,1fr)]"
            >
              <span className="hidden font-mono text-[12px] text-fg-faint tabular-nums md:block md:pt-1.5">
                {String(i + 1).padStart(2, "0")}
              </span>
              <span>
                <span
                  className={`inline-block rounded-md border px-2.5 py-1 font-mono text-[13px] font-medium ${r.tone}`}
                >
                  {r.status}
                </span>
              </span>
              <p className="text-[15px] leading-snug text-fg md:pt-1">{r.by}</p>
              <ul className="flex flex-wrap gap-x-2 gap-y-2 md:col-start-3 lg:col-start-auto">
                {r.examples.map((e) => (
                  <li
                    key={e}
                    className="rounded-full border border-line bg-surface px-2.5 py-0.5 font-mono text-[12px] text-fg-muted"
                  >
                    {e}
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ol>
        <p className="mt-6 max-w-[44rem] text-[14.5px] leading-relaxed text-fg-muted">
          In v0 every contract is <code className="font-mono text-fg">runtime_checked</code>, in
          every build including <code className="font-mono text-fg">--release</code>. From M4,{" "}
          <code className="font-mono text-fg">sakura verify</code> can move a single obligation to{" "}
          <code className="font-mono text-fg">proved</code> and drop only that check.{" "}
          <Link
            href="/docs/tour/contracts"
            className="font-medium text-fg underline decoration-seal underline-offset-4"
          >
            Contracts
          </Link>
        </p>
      </div>
    </section>
  );
}
