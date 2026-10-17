import { Check } from "lucide-react";
import { TokenSpans } from "@/components/token-spans";
import { tokenize } from "@/lib/sakura/highlight";

const FILES = [
  { path: "src/billing/retry.sakura", add: 214, del: 90 },
  { path: "src/billing/ledger.sakura", add: 131, del: 122 },
  { path: "src/billing/charge.sakura", add: 38, del: 11, signature: true },
  { path: "src/billing/webhooks.sakura", add: 156, del: 104 },
  { path: "test/billing_test.sakura", add: 73, del: 61 },
];

const BEFORE =
  "pub fn charge(card: Card, amount: Cents) -> Receipt ! {Payments, Fail<ChargeError>}";
const AFTER =
  "pub fn charge(card: Card, amount: Cents) -> Receipt ! {Http, Payments, Fail<ChargeError>}";

/** One unbroken line from `sm` up; on phones, wrap with a hanging indent. */
const WRAP =
  "min-w-0 whitespace-pre-wrap pl-[4ch] -indent-[4ch] [overflow-wrap:anywhere] sm:whitespace-pre sm:pl-0 sm:indent-0";

export async function Review() {
  const [before, after] = await Promise.all([tokenize(BEFORE), tokenize(AFTER)]);
  return (
    <section className="border-b border-line bg-bg-sunk/50">
      <div className="mx-auto grid max-w-6xl grid-cols-1 gap-14 px-5 py-24 sm:px-8 md:py-32 lg:grid-cols-[minmax(0,25rem)_minmax(0,1fr)] lg:gap-16">
        <div className="lg:sticky lg:top-28 lg:self-start">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Review promises, not diffs.
          </h2>
          <p className="mt-5 max-w-[30rem] text-[17px] leading-relaxed text-pretty text-fg-muted">
            An agent can rewrite six hundred lines before you finish your coffee. Nobody reviews
            that well. In Sakura Lang, everything a function is allowed to do lives in its
            signature, so review shrinks to the lines that change a promise.
          </p>
        </div>

        <figure>
          <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_24px_60px_-34px_oklch(0.2_0.02_255/0.35)]">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-line px-5 py-3.5">
              <p className="font-mono text-[12.5px]">
                <span className="text-fg-muted">agent/</span>billing-retries
              </p>
              <p className="font-mono text-[12px] text-fg-muted tabular-nums">
                5 files <span className="text-ok">+612</span>{" "}
                <span className="text-fail">−388</span>
              </p>
            </div>
            <ul className="divide-y divide-line">
              {FILES.map((f) =>
                f.signature ? (
                  <li key={f.path} className="bg-seal-wash/60">
                    <div className="flex items-center justify-between gap-3 px-5 py-3">
                      <span className="font-mono text-[12.5px]">{f.path}</span>
                      <span className="rounded-full bg-seal px-2.5 py-0.5 text-[11.5px] font-semibold whitespace-nowrap text-seal-ink">
                        <span className="hidden sm:inline">Review: </span>promise changed
                      </span>
                    </div>
                    <div className="relative overflow-x-auto border-y border-line bg-code py-2 font-mono text-[11.5px] leading-[1.8] sm:text-[12px]">
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
                    <p className="px-5 py-3 text-[13.5px] leading-relaxed text-fg">
                      <span className="font-semibold">charge</span> now makes HTTP calls. That is a
                      decision for a person, so the diff surfaces it, and nothing else.
                    </p>
                  </li>
                ) : (
                  <li key={f.path} className="flex items-center justify-between gap-3 px-5 py-3">
                    <span className="font-mono text-[12.5px] text-fg-muted">{f.path}</span>
                    <span className="flex items-center gap-3 font-mono text-[11.5px] tabular-nums">
                      <span className="hidden text-fg-faint sm:inline">
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
          <figcaption className="mt-3 text-[12.5px] text-fg-faint">
            Illustrative review of an agent&apos;s change. The tooling is part of the design, not a
            shipped product.
          </figcaption>
        </figure>
      </div>
    </section>
  );
}
