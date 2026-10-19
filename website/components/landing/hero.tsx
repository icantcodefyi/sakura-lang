import type { CSSProperties } from "react";
import Link from "next/link";
import { ArrowRight, ArrowUpRight } from "lucide-react";
import { GitHubIcon } from "@/components/github-icon";
import { Button } from "@/components/ui/button";
import { tokenize } from "@/lib/sakura/highlight";
import { site } from "@/lib/site";
import { HeroDemo } from "./hero-demo";
import { sample } from "./samples";

export async function Hero() {
  const lines = await tokenize(sample("signup"));
  return (
    <section className="relative isolate overflow-hidden border-b border-border">
      <div aria-hidden className="sakura-sky" />
      <div className="relative mx-auto max-w-6xl px-5 pt-12 pb-20 sm:px-8 md:pt-20 lg:pb-28">
        <Link
          href="/docs/install"
          className="rise group flex max-w-xl items-center gap-3 font-mono whitespace-nowrap text-[12px] text-foreground sm:text-[12.5px]"
        >
          <span className="bg-foreground px-1.5 py-0.5 text-background">alpha.1</span>
          <span
            aria-hidden
            className="h-px min-w-6 flex-1 translate-y-[0.2em] border-b border-dotted border-foreground/50"
          />
          <span>
            Published on npm<span className="hidden sm:inline">, Homebrew and install.sh</span>
          </span>
          <ArrowRight className="size-3.5 shrink-0 transition-transform duration-200 ease-out group-hover:translate-x-0.5" />
        </Link>

        <h1
          className="rise-text mt-7 font-display text-[clamp(3rem,8.4vw,6.4rem)] leading-[0.9] font-semibold tracking-[-0.055em] md:mt-9"
          style={{ "--i": 1 } as CSSProperties}
        >
          Code that keeps <br className="hidden sm:block" />
          its promises.
        </h1>

        <div className="mt-10 grid grid-cols-1 items-start gap-12 md:mt-14 lg:grid-cols-[minmax(0,0.7fr)_minmax(0,1fr)] lg:gap-14">
          <div className="lg:pt-2">
            <p
              className="rise-text max-w-[30rem] text-[clamp(1.05rem,1.35vw,1.2rem)] leading-relaxed text-pretty text-foreground"
              style={{ "--i": 2 } as CSSProperties}
            >
              Sakura Lang is the language agents write. Every signature declares its effects, errors
              and contracts, and the compiler holds the code to them.
            </p>
            <div
              className="rise mt-8 flex flex-wrap items-center gap-3"
              style={{ "--i": 3 } as CSSProperties}
            >
              <Button asChild size="lg" className="group">
                <Link href="/docs/install">
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
            <dl
              className="rise mt-8 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 border-t border-dotted border-foreground/25 pt-4 font-mono text-[12.5px] text-muted-foreground"
              style={{ "--i": 4 } as CSSProperties}
            >
              <dt className="text-foreground">install</dt>
              <dd>
                <code className="text-foreground select-all">
                  npm i -g --ignore-scripts sakuralang
                </code>
              </dd>
              <dt className="text-foreground">agents</dt>
              <dd>
                GET{" "}
                {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
                <a
                  href="/llms.txt"
                  className="text-foreground underline decoration-sakura decoration-[1.5px] underline-offset-4 transition-colors duration-150 hover:text-sakura-text"
                >
                  /llms.txt
                </a>
                , or any page + <span className="text-foreground">.md</span>
              </dd>
            </dl>
          </div>

          <div className="rise xl:-mr-6" style={{ "--i": 2 } as CSSProperties}>
            <HeroDemo lines={lines} />
          </div>
        </div>
      </div>
    </section>
  );
}
