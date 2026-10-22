import type { ReactNode } from "react";
import { WinCallout } from "@/components/bench";
import { callouts, run, targetMedian, times } from "@/lib/bench";
import { Section, TextLink } from "./section";

const C = ({ children }: { children: ReactNode }) => (
  <code className="font-mono text-[0.92em] text-foreground">{children}</code>
);

// Why the numbers come out as they do, in plain words: each is something the compiler does
// today. /docs/benchmarks#why-its-fast has the longer list.
const WHY: { title: string; body: ReactNode }[] = [
  {
    title: "Compiled ahead of time",
    body: "Your program becomes plain JavaScript or WebAssembly (native code is experimental). Types are checked once and leave nothing behind.",
  },
  {
    title: "Memory reused in place",
    body: "On WebAssembly and native, updating a value nothing else holds rewrites it instead of copying it.",
  },
  {
    title: "No hidden copies",
    body: (
      <>
        When an update has to copy, <C>sakura check</C> tells you with warning <C>W8001</C>.
      </>
    ),
  },
  {
    title: "A checker built for the loop",
    body: "The compiler is written in Rust and caches the standard library, so your agent's checks come back in milliseconds.",
  },
];

// Where Sakura is ahead in the blessed baseline (lib/bench.ts finds the wins by rule), with the
// typical program in the lede, so a win never reads as the average.
export function Benchmarks() {
  return (
    <Section
      id="benchmarks"
      ask="Is it fast?"
      title="Fast where your agent waits."
      lede={
        <>
          An agent checks and builds after every edit, and Sakura does both in milliseconds. Running
          code is mixed: a few programs beat hand-written JavaScript, but the typical one still does{" "}
          {times(targetMedian.node)} the work.{" "}
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
      <div className="mt-14 border-t border-border pt-8">
        <h3 className="font-mono text-[12px] tracking-[0.14em] text-sakura-text uppercase">
          Why it&apos;s fast
        </h3>
        <ul className="mt-5 grid gap-x-10 gap-y-6 sm:grid-cols-2 lg:grid-cols-4">
          {WHY.map((w) => (
            <li key={w.title}>
              <p className="text-[15px] font-semibold">{w.title}</p>
              <p className="mt-1.5 text-[14.5px] leading-relaxed text-pretty text-muted-foreground">
                {w.body}
              </p>
            </li>
          ))}
        </ul>
      </div>
      <p className="mt-10 border-t border-border pt-4 font-mono text-[12px] text-faint">
        Work means CPU instructions, counted on an {run.cpu}: unlike seconds, a count repeats from
        run to run. Baseline{" "}
        <a
          href={run.commitUrl}
          className="underline decoration-border underline-offset-[3px] hover:text-foreground"
        >
          {run.short}
        </a>
        , {run.date}, Node {run.engines.node} and Bun {run.engines.bun}.
      </p>
    </Section>
  );
}
