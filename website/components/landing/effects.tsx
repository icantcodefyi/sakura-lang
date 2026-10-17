import Link from "next/link";
import { HandlersDiagram } from "./handlers";

export function Effects() {
  return (
    <section className="border-b border-border">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,26rem)] md:items-end md:gap-16">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Effects are requirements. Handlers answer them.
          </h2>
          <p className="text-[17px] leading-relaxed text-pretty text-muted-foreground">
            <code className="font-mono text-[15px] text-foreground">signup</code> says it needs a
            database, a clock and a way to fail. It never says which. Whoever calls it installs
            handlers, so a test swaps the database for memory with no mocking library and no change
            to the code.
          </p>
        </div>
        <div className="mt-12">
          <HandlersDiagram />
        </div>
        <p className="mt-4 flex flex-wrap gap-x-6 gap-y-2 text-[14.5px]">
          <Link
            href="/docs/tour/effects"
            className="font-medium underline decoration-sakura underline-offset-4"
          >
            Effects
          </Link>
          <Link
            href="/docs/tour/handlers"
            className="font-medium underline decoration-sakura underline-offset-4"
          >
            Handlers
          </Link>
          <span className="text-faint">
            Handlers are tail-resumptive: a clause returns the result once. No resume, no Async.
          </span>
        </p>
      </div>
    </section>
  );
}
