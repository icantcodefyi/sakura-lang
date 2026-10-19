import { Check } from "lucide-react";
import { TokenSpans } from "@/components/token-spans";
import { tokenize } from "@/lib/sakura/highlight";
import { sample } from "./samples";
import { WINDOW, WindowBar } from "./window";

const FILES = [
  { path: "src/billing/retry.sak", add: 214, del: 90 },
  { path: "src/billing/ledger.sak", add: 131, del: 122 },
  { path: "src/billing/charge.sak", add: 38, del: 11, signature: true },
  { path: "src/billing/webhooks.sak", add: 156, del: 104 },
  { path: "test/billing_test.sak", add: 73, del: 61 },
];

// The removed line is the checked sample without the effect it adds.
const AFTER = sample("charge");
const BEFORE = AFTER.replace("{Http, ", "{");

/** One unbroken line from `sm` up; on phones, wrap with a hanging indent. */
const WRAP =
  "min-w-0 whitespace-pre-wrap pl-[4ch] -indent-[4ch] [overflow-wrap:anywhere] sm:whitespace-pre sm:pl-0 sm:indent-0";

export async function Review() {
  const [before, after] = await Promise.all([tokenize(BEFORE), tokenize(AFTER)]);
  return (
    <section className="border-b border-border bg-muted/50">
      <div className="mx-auto grid max-w-6xl grid-cols-1 gap-14 px-5 py-24 sm:px-8 md:py-32 lg:grid-cols-[minmax(0,25rem)_minmax(0,1fr)] lg:gap-16">
        <div className="lg:sticky lg:top-28 lg:self-start">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Review promises, not diffs.
          </h2>
          <p className="mt-5 max-w-[30rem] text-[17px] leading-relaxed text-pretty text-muted-foreground">
            An agent can rewrite six hundred lines before you finish your coffee. Nobody reviews
            that well. In Sakura Lang, everything a function is allowed to do lives in its
            signature, so review shrinks to the lines that change a promise.
          </p>
        </div>

        <figure>
          <div className={WINDOW}>
            <WindowBar className="flex-wrap justify-between gap-2 py-1.5 sm:px-5">
              <p className="font-mono text-[12.5px]">
                <span className="text-muted-foreground">agent/</span>billing-retries
              </p>
              <p className="font-mono text-[12px] text-muted-foreground tabular-nums">
                5 files <span className="text-ok">+612</span>{" "}
                <span className="text-fail">−388</span>
              </p>
            </WindowBar>
            <ul className="divide-y divide-border">
              {FILES.map((f) =>
                f.signature ? (
                  <li key={f.path} className="bg-sakura-wash/60">
                    <div className="flex items-center justify-between gap-3 px-5 py-3">
                      <span className="font-mono text-[12.5px]">{f.path}</span>
                      <span className="rounded-full bg-sakura px-2.5 py-0.5 text-[11.5px] font-semibold whitespace-nowrap text-ink">
                        <span className="hidden sm:inline">Review: </span>promise changed
                      </span>
                    </div>
                    <div className="relative overflow-x-auto border-y border-border bg-code py-2 font-mono text-[11.5px] leading-[1.8] sm:text-[12px]">
                      <div className="sm:min-w-max">
                        <p className="flex bg-fail-wash pr-4 sm:pr-5">
                          <span
                            aria-hidden
                            className="w-8 shrink-0 text-center text-fail select-none"
                          >
                            −
                          </span>
                          <span className="sr-only">Removed: </span>
                          <span className={WRAP}>
                            <TokenSpans line={before[0]} />
                          </span>
                        </p>
                        <p className="flex bg-ok-wash pr-4 sm:pr-5">
                          <span
                            aria-hidden
                            className="w-8 shrink-0 text-center text-ok select-none"
                          >
                            +
                          </span>
                          <span className="sr-only">Added: </span>
                          <span className={WRAP}>
                            <TokenSpans line={after[0]} />
                          </span>
                        </p>
                      </div>
                    </div>
                    <p className="px-5 py-3 text-[13.5px] leading-relaxed text-foreground">
                      <span className="font-semibold">charge</span> now makes HTTP calls. That is a
                      decision for a person, so the diff surfaces it, and nothing else.
                    </p>
                  </li>
                ) : (
                  <li key={f.path} className="flex items-center justify-between gap-3 px-5 py-3">
                    <span className="font-mono text-[12.5px] text-muted-foreground">{f.path}</span>
                    <span className="flex items-center gap-3 font-mono text-[11.5px] tabular-nums">
                      <span className="hidden text-faint sm:inline">
                        <span className="text-ok">+{f.add}</span>{" "}
                        <span className="text-fail">−{f.del}</span>
                      </span>
                      <span className="inline-flex items-center gap-1 text-ok">
                        <Check className="size-3.5" strokeWidth={2.25} aria-hidden />
                        <span className="whitespace-nowrap">
                          <span className="hidden sm:inline">signatures </span>kept
                        </span>
                      </span>
                    </span>
                  </li>
                ),
              )}
            </ul>
          </div>
          <figcaption className="mt-3 text-[12.5px] text-faint">
            Illustrative review screen. The flagged row is real: MCP{" "}
            <code className="font-mono">check_patch</code> returns this change as{" "}
            <code className="font-mono">interface_delta</code> with{" "}
            <code className="font-mono">effects_changed: true</code>, and with{" "}
            <code className="font-mono">frozen_specs</code> set it is reported as{" "}
            <code className="font-mono">V8020</code>.
          </figcaption>
        </figure>
      </div>
    </section>
  );
}
