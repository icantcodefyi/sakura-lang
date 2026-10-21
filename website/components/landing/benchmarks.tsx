import Link from "next/link";
import { WinCallout } from "@/components/bench";
import { callouts, envelopeTally, run, targetMedian, targets, times } from "@/lib/bench";

// Where Sakura is ahead in the blessed baseline (lib/bench.ts finds the wins by rule), then the
// whole picture beside it, so a win never reads as the average: the median against hand JS,
// the ceilings met and the cross-language envelope, misses stated as misses.
const within = (e: "node" | "bun") => targets.filter((t) => t[e] <= t.ceiling[e]).length;

const READINGS = [
  {
    value: times(targetMedian.node),
    label: "median over hand-written JS on Node",
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
            Where Sakura is fast.
          </h2>
          <p className="max-w-sm text-[15px] leading-relaxed text-muted-foreground">
            Instructions retired per call on an {run.cpu}, against the same program written by hand
            in JavaScript on the same engine, from the blessed baseline. These are the cells where
            Sakura is ahead; most are not, and the line below says so.{" "}
            <Link
              href="/docs/benchmarks"
              className="font-medium text-foreground underline decoration-sakura underline-offset-4"
            >
              Every number
            </Link>
          </p>
        </div>
        <div className="mt-14 border-t border-border">
          <ul className="grid sm:-mx-6 sm:grid-cols-2 lg:grid-cols-4">
            {callouts.map((w) => (
              <li
                key={w.id}
                className="border-b border-border py-7 last:border-b-0 sm:px-6 sm:nth-[2n]:border-l lg:border-b-0 lg:not-first:border-l"
              >
                <WinCallout win={w} href="/docs/benchmarks" />
              </li>
            ))}
          </ul>
        </div>
        <dl className="mt-10 grid gap-x-8 gap-y-4 border-t border-border pt-6 sm:grid-cols-3">
          {READINGS.map((r) => (
            <div key={r.label} className="flex flex-col">
              <dt className="text-[13.5px] leading-snug text-muted-foreground">{r.label}</dt>
              <dd className="order-first font-display text-[1.6rem] leading-tight font-semibold tracking-[-0.02em] tabular-nums">
                {r.value}
              </dd>
              <dd className="font-mono text-[12px] text-faint">{r.target}</dd>
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
          , {run.date}, Node {run.engines.node} and Bun {run.engines.bun}. Counts, not times.
        </p>
      </div>
    </section>
  );
}
