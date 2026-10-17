import type { CSSProperties } from "react";
import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { GitHubIcon } from "@/components/github-icon";
import { tokenize } from "@/lib/vow/highlight";
import { site } from "@/lib/site";
import { HERO_CODE } from "./hero-code";
import { HeroDemo } from "./hero-demo";

export async function Hero() {
  const lines = await tokenize(HERO_CODE);
  return (
    <section className="relative overflow-hidden border-b border-line">
      <Ledger />
      <div className="relative mx-auto grid max-w-6xl items-center gap-12 px-5 pt-14 pb-20 sm:px-8 md:pt-20 lg:grid-cols-[minmax(0,0.86fr)_minmax(0,1fr)] lg:gap-10 lg:pb-28">
        <div>
          <Link
            href="/docs/roadmap"
            className="rise group inline-flex items-center gap-2 rounded-full border border-line bg-surface/70 py-1 pr-3 pl-1 text-[12.5px] text-fg-muted transition-colors duration-150 hover:border-line-strong hover:text-fg"
          >
            <span className="rounded-full bg-seal-wash px-2 py-0.5 font-medium text-seal-text">Pre-alpha</span>
            In early design, no release yet
            <ArrowRight className="size-3 transition-transform duration-200 group-hover:translate-x-0.5" />
          </Link>
          <h1
            className="rise mt-7 font-display text-[clamp(2.6rem,4.7vw,3.75rem)] leading-[1.02] font-semibold tracking-[-0.035em] text-balance"
            style={{ "--i": 1 } as CSSProperties}
          >
            Code that keeps <br className="hidden sm:block" />
            its promises.
          </h1>
          <p
            className="rise mt-6 max-w-[34rem] text-[clamp(1.05rem,1.4vw,1.2rem)] leading-relaxed text-pretty text-fg-muted"
            style={{ "--i": 2 } as CSSProperties}
          >
            Vow is the language agents write. Every signature declares its effects, errors and contracts, and the
            compiler holds the code to them.
          </p>
          <div className="rise mt-9 flex flex-wrap items-center gap-3" style={{ "--i": 3 } as CSSProperties}>
            <Link
              href="/docs"
              className="group inline-flex h-11 items-center gap-2 rounded-full bg-seal pr-2 pl-5 text-[15px] font-semibold text-seal-ink shadow-[inset_0_1px_0_oklch(1_0_0/0.35),0_8px_24px_-10px_oklch(0.7_0.15_70/0.7)] transition-transform duration-150 active:scale-[0.97]"
            >
              Read the docs
              <span className="grid size-7 place-items-center rounded-full bg-seal-ink/10 transition-transform duration-200 group-hover:translate-x-0.5">
                <ArrowRight className="size-3.5" />
              </span>
            </Link>
            <a
              href={site.github}
              className="inline-flex h-11 items-center gap-2 rounded-full border border-line-strong px-5 text-[15px] font-medium text-fg transition-[background-color,transform] duration-150 hover:bg-bg-sunk active:scale-[0.97]"
            >
              <GitHubIcon className="size-4" />
              View on GitHub
            </a>
          </div>
        </div>

        <div className="rise lg:-mr-6" style={{ "--i": 2 } as CSSProperties}>
          <HeroDemo lines={lines} />
        </div>
      </div>
    </section>
  );
}

/** Faint ruled-ledger background: the paper a contract is written on. */
function Ledger() {
  return (
    <div
      aria-hidden
      className="pointer-events-none absolute inset-0 [mask-image:linear-gradient(to_bottom,black,transparent_85%)] opacity-60 dark:opacity-40"
      style={{
        backgroundImage:
          "repeating-linear-gradient(to bottom, transparent 0 31px, var(--line) 31px 32px)",
      }}
    />
  );
}
