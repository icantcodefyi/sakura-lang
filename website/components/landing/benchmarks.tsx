import type { ReactNode } from "react";
import { WinCallout } from "@/components/bench";
import { CountUp } from "@/components/count-up";
import {
  callouts,
  hostedRun,
  nativeHeadline,
  nativeSummary,
  run,
  targetMedian,
  times,
  factor,
} from "@/lib/bench";
import { Section, TextLink } from "./section";

const C = ({ children }: { children: ReactNode }) => (
  <code className="font-mono text-[0.92em] text-foreground">{children}</code>
);

/** `binary_trees` in a computed sentence as code. */
const inline = (s: string) =>
  s
    .split(/(`[^`]+`)/)
    .map((part, i) => (part.startsWith("`") ? <C key={i}>{part.slice(1, -1)}</C> : part));

// What the native backend does today, in plain words. /docs/benchmarks has the longer list,
// with what still holds it back.
const WHY: { title: string; body: ReactNode }[] = [
  {
    title: "Cranelift machine code",
    body: "--target native compiles straight to machine code. Typed numbers stay plain machine numbers.",
  },
  {
    title: "Memory reused in place",
    body: "When nothing else holds a value, an update rewrites it where it is instead of copying it (Perceus).",
  },
  {
    title: "Borrowing, not copying",
    body: "A function that only reads its argument borrows it, so the call does no reference-counting work.",
  },
  {
    title: "No collector pauses",
    body: "Reference counting frees memory the moment it is last used: there is no garbage collector to stop the program.",
  },
];

const c = nativeSummary.find((r) => r.lang === "c") ?? nativeSummary[0];

// Native Sakura against each language, all of them, typical first, then its closest and its
// furthest program: found by rule from the counted files, never picked.
export function Benchmarks() {
  return (
    <Section
      id="benchmarks"
      ask="Is it fast?"
      title="Native Sakura, counted against C, Rust, Go, OCaml and Koka."
      lede={
        <>
          The same programs in six languages, each one&apos;s work counted on one machine. For the
          typical program native Sakura does {factor(c.typical)} the work of {c.name}. Its closest
          and furthest results are both below.{" "}
          <TextLink href="/docs/benchmarks">Every number</TextLink>
        </>
      }
    >
      <ul className="grid border-t border-border sm:-mx-6 sm:grid-cols-2 lg:grid-cols-5">
        {nativeSummary.map((r) => (
          <li
            key={r.lang}
            className="border-b border-border py-7 sm:px-6 sm:nth-[2n]:border-l lg:border-b-0 lg:not-first:border-l"
          >
            <p className="font-mono text-[12px] tracking-[0.14em] text-muted-foreground uppercase">
              vs {r.name}
            </p>
            <p className="mt-3 font-display text-[clamp(2.2rem,4vw,3rem)] leading-none font-semibold tracking-[-0.03em]">
              <CountUp value={factor(r.typical)} />
            </p>
            <p className="mt-3 text-[15px] leading-snug text-balance">
              the work, for the typical program
            </p>
            <p className="mt-4 text-[14px] leading-relaxed text-pretty text-muted-foreground">
              <span className="text-foreground">
                {inline(nativeHeadline(r.closest, r.name).replace(/^./, (x) => x.toUpperCase()))}
              </span>
              . Furthest: {factor(r.furthest.ratio)} on <C>{r.furthest.program}</C>.
            </p>
          </li>
        ))}
      </ul>
      <div className="mt-14 border-t border-border pt-8">
        <h3 className="font-mono text-[12px] tracking-[0.14em] text-sakura-text uppercase">
          Why native Sakura is fast
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
      <div className="mt-14 border-t border-border pt-8">
        <h3 className="font-mono text-[12px] tracking-[0.14em] text-sakura-text uppercase">
          The JavaScript and WebAssembly builds
        </h3>
        <p className="mt-3 max-w-2xl text-[15px] leading-relaxed text-pretty text-muted-foreground">
          Against the same programs written by hand in JavaScript, the typical one does{" "}
          {times(targetMedian.node)} the work on Node. Where Sakura is ahead:
        </p>
        <ul className="mt-6 grid border-t border-border sm:-mx-6 sm:grid-cols-2 lg:grid-cols-4">
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
      <p className="mt-10 border-t border-border pt-4 font-mono text-[12px] text-faint">
        Work means CPU instructions per call, counted on an {run.cpu}: unlike seconds, a count
        repeats from run to run. Baseline{" "}
        <a
          href={run.commitUrl}
          className="underline decoration-border underline-offset-[3px] hover:text-foreground"
        >
          {run.short}
        </a>
        , {run.date}. Wall time on a GitHub runner:{" "}
        <a
          href={hostedRun.url}
          className="underline decoration-border underline-offset-[3px] hover:text-foreground"
        >
          run {hostedRun.id}
        </a>
        .
      </p>
    </Section>
  );
}
