import Link from "next/link";
import { envelopeTally, run, targetMedian, targets, times } from "@/lib/bench";

// Three readings of the blessed baseline (lib/bench.ts), each beside its target, so a miss reads
// as a miss. No cross-language ratio: the envelope is stated as cells met and missed.
const within = (e: "node" | "bun") => targets.filter((t) => t[e] <= t.ceiling[e]).length;

const READINGS = [
  {
    value: times(targetMedian.node),
    label: "median, Sakura over hand-written JS on Node",
    target: `target ≤ ${targetMedian.ceiling.toFixed(1)}×; Bun ${times(targetMedian.bun)}`,
  },
  {
    value: `${within("node")} of ${targets.length}`,
    label: "programs within their ceiling on Node",
    target: `${within("bun")} of ${targets.length} on Bun`,
  },
  {
    value: `${envelopeTally.miss} of ${envelopeTally.cells}`,
    label: "cross-language envelope cells missed",
    target: "carried; not a release gate",
  },
];

export function Benchmarks() {
  return (
    <section id="benchmarks" className="border-b border-border">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="flex flex-col justify-between gap-6 md:flex-row md:items-end">
          <h2 className="max-w-xl font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Measured, not promised.
          </h2>
          <p className="max-w-sm text-[15px] leading-relaxed text-muted-foreground">
            Instructions retired per call on an {run.cpu}, against the same program written by hand
            in JavaScript on the same engine. Counts, not times, and short of the targets today.{" "}
            <Link
              href="/docs/benchmarks"
              className="font-medium text-foreground underline decoration-sakura underline-offset-4"
            >
              Every number
            </Link>
          </p>
        </div>
        <dl className="mt-14 grid border-t border-border md:grid-cols-3">
          {READINGS.map((r) => (
            <div
              key={r.label}
              className="flex flex-col border-b border-border py-6 md:border-b-0 md:py-8 md:not-first:border-l md:not-first:pl-8 md:not-last:pr-8"
            >
              <dt className="mt-3 text-[15px] leading-snug">{r.label}</dt>
              <dd className="order-first font-display text-[clamp(2.2rem,4vw,3rem)] leading-none font-semibold tracking-[-0.03em] tabular-nums">
                {r.value}
              </dd>
              <dd className="mt-1 font-mono text-[12.5px] text-faint">{r.target}</dd>
            </div>
          ))}
        </dl>
        <p className="mt-6 font-mono text-[12px] text-faint">
          Blessed baseline at{" "}
          <a
            href={run.commitUrl}
            className="underline decoration-border underline-offset-[3px] hover:text-foreground"
          >
            {run.short}
          </a>
          , {run.date}, Node {run.engines.node} and Bun {run.engines.bun}.
        </p>
      </div>
    </section>
  );
}
