import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { Seal } from "@/components/seal";

export function Closing() {
  return (
    <section className="px-5 py-24 sm:px-8 md:py-28">
      <div className="relative mx-auto max-w-6xl overflow-hidden rounded-[1.75rem] bg-[oklch(0.19_0.012_255)] px-6 py-14 text-[oklch(0.95_0.006_255)] ring-1 ring-white/10 sm:px-12 md:py-20 dark:bg-card">
        <div className="relative max-w-2xl">
          <h2 className="font-display text-[clamp(2rem,4.2vw,3.3rem)] leading-[1.02] font-semibold tracking-[-0.03em] text-balance">
            Read the design. Then hold us to it.
          </h2>
          <p className="mt-5 max-w-[34rem] text-[17px] leading-relaxed text-[oklch(0.78_0.012_255)]">
            alpha.1 is one command away. The docs are written for people and for the agents that
            work with them, and every page is one fetch away as Markdown.
          </p>
          <div className="mt-9 flex flex-wrap items-center gap-3">
            <Link
              href="/docs/install"
              className="group inline-flex h-11 items-center gap-2 rounded-full bg-sakura pr-2 pl-5 text-[15px] font-semibold text-ink transition-transform duration-150 active:scale-[0.97]"
            >
              Install alpha.1
              <span className="grid size-7 place-items-center rounded-full bg-ink/10 transition-transform duration-200 group-hover:translate-x-0.5">
                <ArrowRight className="size-3.5" />
              </span>
            </Link>
            <Link
              href="/docs"
              className="inline-flex h-11 items-center gap-2 rounded-full border border-white/20 px-5 text-[15px] font-medium transition-[background-color,transform] duration-150 hover:bg-white/10 active:scale-[0.97]"
            >
              Read the docs
            </Link>
          </div>
        </div>
        <Seal className="pointer-events-none mt-10 ml-auto block size-28 rotate-[-14deg] lg:absolute lg:top-1/2 lg:right-12 lg:mt-0 lg:size-[17rem] lg:-translate-y-1/2" />
      </div>
    </section>
  );
}
