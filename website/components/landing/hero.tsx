import type { CSSProperties } from "react";
import Link from "next/link";
import { ArrowRight, ArrowUpRight } from "lucide-react";
import { GitHubIcon } from "@/components/github-icon";
import { Button } from "@/components/ui/button";
import { releaseLabel } from "@/lib/release";
import { site } from "@/lib/site";
import { FallingPetals } from "@/components/site-footer";
import { Tree } from "./tree";

/** The hero's falling petals below lg: left edge, drift, fall time, phase and size. */
const PETALS = [
  ["8%", "-40px", "19s", "-3s", 0.9],
  ["27%", "-70px", "23s", "-15s", 1.05],
  ["46%", "-55px", "21s", "-8s", 0.95],
  ["63%", "-90px", "24s", "-19s", 1.1],
  ["78%", "-60px", "18s", "-11s", 0.9],
  ["92%", "-80px", "22s", "-5s", 1],
] as const;

export function Hero() {
  return (
    // From lg up the hero keeps the poster's shape (1800 × 870, lib/hero-tree.ts) as the screen
    // widens, up to the screen's height under the header, so `cover` never crops the great limb.
    <section className="relative overflow-hidden border-b border-border lg:flex lg:min-h-[min(48.33vw,calc(100svh-3.5rem))] lg:flex-col lg:justify-center">
      {/* From lg up the tree fills the hero behind the copy, its limb arching over it. */}
      <div className="absolute inset-0 hidden lg:block">
        <Tree className="size-full" />
      </div>
      {/* Below lg, the poster's right-hand half, the trunk and the limb, grows from the bottom
          right under the copy and fades into the page; petals fall through the whole hero. */}
      <div
        className="hero-tree absolute inset-x-0 bottom-0 h-[min(97vw,30rem)] lg:hidden"
        aria-hidden
      >
        <div className="tree-poster size-full" />
      </div>
      <div className="[--fall:1100px] lg:hidden">
        <FallingPetals petals={PETALS} />
      </div>
      <div className="relative z-10 mx-auto grid w-full max-w-6xl items-center gap-x-6 px-5 pt-12 sm:px-8 md:pt-16 lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
        <div className="pb-4 lg:pb-24">
          <HeroCopy />
        </div>
        {/* Room for the trunk under the copy; from lg up the box keeps the hero's height. */}
        <div
          className="-mx-5 h-[min(84vw,26rem)] sm:mx-0 lg:aspect-[25/21] lg:h-auto xl:-mr-10"
          aria-hidden
        />
      </div>
    </section>
  );
}

/** The hero's words and actions. */
function HeroCopy() {
  return (
    <>
      <Link
        href="/docs/install"
        className="rise group flex max-w-md items-center gap-3 font-mono whitespace-nowrap text-[12px] text-foreground sm:text-[12.5px]"
      >
        <span className="bg-foreground px-1.5 py-0.5 text-background">{releaseLabel()}</span>
        <span
          aria-hidden
          className="h-px min-w-6 flex-1 translate-y-[0.2em] border-b border-dotted border-foreground/50"
        />
        <span>
          Published on npm<span className="hidden sm:inline">, Homebrew and install.sh</span>
        </span>
        <ArrowRight className="size-3.5 shrink-0 transition-transform duration-200 ease-out group-hover:translate-x-0.5" />
      </Link>

      <p
        className="rise-text mt-8 font-mono text-[12px] tracking-[0.14em] text-sakura-text uppercase sm:text-[12.5px]"
        style={{ "--i": 1 } as CSSProperties}
      >
        Agent-first programming language
      </p>
      <h1
        className="rise-text mt-3 font-display text-[clamp(2.4rem,4.6vw,3.9rem)] leading-[0.92] text-balance font-semibold tracking-[-0.055em]"
        style={{ "--i": 1 } as CSSProperties}
      >
        The programming language built for agents.
      </h1>
      <p
        className="rise-text mt-7 max-w-[31rem] text-[clamp(1.05rem,1.35vw,1.2rem)] leading-relaxed text-pretty"
        style={{ "--i": 2 } as CSSProperties}
      >
        It compiles to JavaScript and WebAssembly. Effects, errors and contracts live in every
        signature; the compiler checks them, and{" "}
        <code className="font-mono text-[0.9em]">sakura verify</code> proves contracts with Z3.
      </p>
      <div
        className="rise mt-8 flex flex-wrap items-center gap-3"
        style={{ "--i": 3 } as CSSProperties}
      >
        <Button asChild size="lg" className="group">
          <Link href="#install">
            Install
            <ArrowRight
              data-icon="inline-end"
              className="transition-transform duration-200 ease-out group-hover:translate-x-0.5"
            />
          </Link>
        </Button>
        <Button
          asChild
          size="lg"
          variant="outline"
          className="border-foreground/25 bg-background/60 hover:bg-background"
        >
          <Link href="/docs">Read the docs</Link>
        </Button>
        <Button
          asChild
          size="lg"
          variant="ghost"
          className="group px-3 text-muted-foreground hover:bg-transparent hover:text-foreground max-sm:w-11 max-sm:border max-sm:border-foreground/25 max-sm:px-0"
        >
          <a href={site.github}>
            <GitHubIcon className="size-4" />
            <span className="sr-only sm:not-sr-only">GitHub</span>
            <ArrowUpRight className="hidden size-3.5 transition-transform duration-200 ease-out group-hover:translate-x-0.5 group-hover:-translate-y-0.5 sm:block" />
          </a>
        </Button>
      </div>
      <p
        className="rise mt-7 font-mono text-[12.5px] text-muted-foreground"
        style={{ "--i": 4 } as CSSProperties}
      >
        <span className="text-faint select-none">$ </span>
        <code className="text-foreground select-all">npm i -g sakuralang@beta</code>
        <span className="mx-2 text-faint">·</span>
        agents: GET{" "}
        {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
        <a
          href="/llms.txt"
          className="text-foreground underline decoration-sakura decoration-[1.5px] underline-offset-4 transition-colors duration-150 hover:text-sakura-text"
        >
          /llms.txt
        </a>
      </p>
    </>
  );
}
