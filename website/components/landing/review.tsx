import { Check } from "lucide-react";
import { TokenSpans } from "@/components/token-spans";
import { tokenize } from "@/lib/vow/highlight";

const FILES = [
  { path: "src/billing/retry.vow", add: 214, del: 90 },
  { path: "src/billing/ledger.vow", add: 131, del: 122 },
  { path: "src/billing/charge.vow", add: 38, del: 11, signature: true },
  { path: "src/billing/webhooks.vow", add: 156, del: 104 },
  { path: "test/billing_test.vow", add: 73, del: 61 },
];

const BEFORE = "pub fn charge(card: Card, amount: Cents) -> Receipt ! {Payments, Fail<ChargeError>}";
const AFTER = "pub fn charge(card: Card, amount: Cents) -> Receipt ! {Payments, Net, Fail<ChargeError>}";

export async function Review() {
  const [before, after] = await Promise.all([tokenize(BEFORE), tokenize(AFTER)]);
  return (
    <section className="border-b border-line">
      <div className="mx-auto grid max-w-6xl gap-14 px-5 py-24 sm:px-8 md:py-32 lg:grid-cols-[minmax(0,25rem)_minmax(0,1fr)] lg:gap-16">
        <div className="lg:sticky lg:top-28 lg:self-start">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            The signature is the contract.
          </h2>
          <p className="mt-5 max-w-[30rem] text-[17px] leading-relaxed text-pretty text-fg-muted">
            An agent can rewrite six hundred lines before you finish your coffee. Nobody reviews that well. In Vow,
            everything a function is allowed to do lives in its signature, so review shrinks to the lines that change a
            promise.
          </p>
          <dl className="mt-10 grid max-w-[30rem] gap-5 text-[15px]">
            <div className="grid grid-cols-[7.5rem_1fr] gap-4 border-t border-line pt-5">
              <dt className="font-medium">Humans</dt>
              <dd className="text-fg-muted">read signatures: inputs, outputs, effects, errors, contracts.</dd>
            </div>
            <div className="grid grid-cols-[7.5rem_1fr] gap-4 border-t border-line pt-5">
              <dt className="font-medium">The compiler</dt>
              <dd className="text-fg-muted">checks every body against its signature, and refuses to build if one breaks its word.</dd>
            </div>
          </dl>
        </div>

        <figure>
          <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_24px_60px_-34px_oklch(0.2_0.02_255/0.35)]">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-line px-5 py-3.5">
              <p className="font-mono text-[12.5px]">
                <span className="text-fg-muted">agent/</span>billing-retries
              </p>
              <p className="font-mono text-[12px] text-fg-muted tabular-nums">
                5 files <span className="text-ok">+612</span> <span className="text-fail">−388</span>
              </p>
            </div>
            <ul className="divide-y divide-line">
              {FILES.map((f) =>
                f.signature ? (
                  <li key={f.path} className="bg-seal-wash/60">
                    <div className="flex items-center justify-between gap-3 px-5 py-3">
                      <span className="font-mono text-[12.5px]">{f.path}</span>
                      <span className="rounded-full bg-seal px-2.5 py-0.5 text-[11.5px] font-semibold text-seal-ink">
                        Review: promise changed
                      </span>
                    </div>
                    <div className="overflow-x-auto border-y border-line bg-code py-2 font-mono text-[11.5px] leading-[1.8] sm:text-[12px]">
                      <div className="min-w-max">
                        <p className="flex bg-fail-wash pr-5 whitespace-pre">
                          <span aria-hidden className="w-8 shrink-0 text-center text-fail select-none">−</span>
                          <span className="sr-only">Removed: </span>
                          <TokenSpans line={before[0]} />
                        </p>
                        <p className="flex bg-ok-wash pr-5 whitespace-pre">
                          <span aria-hidden className="w-8 shrink-0 text-center text-ok select-none">+</span>
                          <span className="sr-only">Added: </span>
                          <TokenSpans line={after[0]} />
                        </p>
                      </div>
                    </div>
                    <p className="px-5 py-3 text-[13.5px] leading-relaxed text-fg">
                      <span className="font-semibold">charge</span> now talks to the network. That is a decision for a
                      person, so the diff surfaces it, and nothing else.
                    </p>
                  </li>
                ) : (
                  <li key={f.path} className="flex items-center justify-between gap-3 px-5 py-3">
                    <span className="font-mono text-[12.5px] text-fg-muted">{f.path}</span>
                    <span className="flex items-center gap-3 font-mono text-[11.5px] tabular-nums">
                      <span className="hidden text-fg-faint sm:inline">
                        <span className="text-ok">+{f.add}</span> <span className="text-fail">−{f.del}</span>
                      </span>
                      <span className="inline-flex items-center gap-1 text-ok">
                        <Check className="size-3.5" strokeWidth={2.25} aria-hidden />
                        signatures kept
                      </span>
                    </span>
                  </li>
                ),
              )}
            </ul>
          </div>
          <figcaption className="mt-3 text-[12.5px] text-fg-faint">
            Illustrative review of an agent&apos;s change. The tooling is part of the design, not a shipped product.
          </figcaption>
        </figure>
      </div>
    </section>
  );
}
