import { WinCallout } from "@/components/bench";
import { callouts, run, targetMedian, times } from "@/lib/bench";
import { Section, TextLink } from "./section";

// Where Sakura is ahead in the blessed baseline (lib/bench.ts finds the wins by rule), with the
// median beside them, so a win never reads as the average.
export function Benchmarks() {
  return (
    <Section
      id="benchmarks"
      ask="Is it fast?"
      title="Where Sakura is fast."
      lede={
        <>
          The cells of the blessed baseline where Sakura is ahead, counted on an {run.cpu}. Most are
          not: across the target programs, the median Sakura program runs {times(targetMedian.node)}{" "}
          the instructions of hand-written JavaScript on Node.{" "}
          <TextLink href="/docs/benchmarks">Every number</TextLink>
        </>
      }
    >
      <ul className="grid border-t border-border sm:-mx-6 sm:grid-cols-2 lg:grid-cols-4">
        {callouts.map((w) => (
          <li
            key={w.id}
            className="border-b border-border py-7 last:border-b-0 sm:px-6 sm:nth-[2n]:border-l lg:border-b-0 lg:not-first:border-l"
          >
            <WinCallout win={w} href="/docs/benchmarks" count />
          </li>
        ))}
      </ul>
      <p className="mt-6 border-t border-border pt-4 font-mono text-[12px] text-faint">
        Blessed baseline at{" "}
        <a
          href={run.commitUrl}
          className="underline decoration-border underline-offset-[3px] hover:text-foreground"
        >
          {run.short}
        </a>
        , {run.date}, Node {run.engines.node} and Bun {run.engines.bun}. Counts, not times.
      </p>
    </Section>
  );
}
