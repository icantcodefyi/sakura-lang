import type { ReactNode } from "react";
import { Seal } from "@/components/seal";
import { TokenSpans } from "@/components/token-spans";
import { tokenize } from "@/lib/vow/highlight";

// One signature, in the formatter's multi-line form (design §12.5), read clause by clause.
const SIGNATURE = `fn withdraw(acct: Account, amount: Cents)
  -> Account
  ! {Ledger, Fail<Overdrawn | Frozen>}
  requires positive: amount > 0
  ensures result.balance == acct.balance - amount`;

const C = ({ children }: { children: ReactNode }) => (
  <code className="font-mono text-[0.9em] text-fg">{children}</code>
);

const NOTES: { label: string; body: ReactNode }[] = [
  {
    label: "Takes",
    body: (
      <>
        Typed inputs. <C>Cents</C> is a refinement: an <C>Int</C> that is never negative.
      </>
    ),
  },
  { label: "Returns", body: "The updated account. Nothing else comes back." },
  {
    label: "May do",
    body: (
      <>
        Only <C>Ledger</C>: no network, files or clock. It fails in exactly two named ways.
      </>
    ),
  },
  {
    label: "Requires",
    body: (
      <>
        The caller&apos;s duty. Breaking it is the caller&apos;s defect, <C>D020</C>.
      </>
    ),
  },
  {
    label: "Ensures",
    body: (
      <>
        The promise, checked on every return in every build, <C>D021</C>.
      </>
    ),
  },
];

export async function Anatomy() {
  const lines = await tokenize(SIGNATURE);
  return (
    <section className="border-b border-line">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,22rem)] md:items-end md:gap-16">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            The signature is the contract.
          </h2>
          <p className="text-[17px] leading-relaxed text-pretty text-fg-muted">
            Five lines a person can review. The compiler holds the body to every one of them.
          </p>
        </div>

        {/* The contract, on ruled paper, with the compiler's seal on the signature line. */}
        <figure className="relative mt-12 overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_24px_60px_-40px_oklch(0.2_0.02_255/0.4)]">
          <div className="flex items-center justify-between border-b border-line px-5 py-3 font-mono text-[11.5px] text-fg-faint sm:px-8">
            <span>src/accounts.vow</span>
            <span className="hidden sm:inline">hover a clause</span>
          </div>
          <ol className="py-3 sm:py-5">
            {lines.map((line, i) => (
              <li
                key={i}
                className="group grid grid-cols-[1.75rem_minmax(0,1fr)] gap-x-3 px-5 py-2 transition-colors duration-150 hover:bg-seal-wash/70 sm:px-8 lg:grid-cols-[1.75rem_minmax(0,1.25fr)_minmax(0,1fr)] lg:items-baseline lg:gap-x-6"
              >
                <span
                  aria-hidden
                  className="row-span-2 grid size-6 place-items-center self-start rounded-full border border-line-strong font-mono text-[11px] text-fg-muted tabular-nums transition-colors duration-150 group-hover:border-seal group-hover:bg-seal group-hover:text-seal-ink lg:row-span-1 lg:self-baseline"
                >
                  {i + 1}
                </span>
                <code className="font-mono text-[12.5px] leading-relaxed [overflow-wrap:anywhere] whitespace-pre-wrap sm:text-[14.5px] sm:whitespace-pre">
                  <TokenSpans line={line} />
                </code>
                <p className="mt-1 text-[14px] leading-snug text-fg-muted lg:mt-0 lg:flex lg:items-baseline lg:gap-3">
                  <span
                    aria-hidden
                    className="hidden h-px min-w-6 flex-1 translate-y-[-0.3em] bg-line-strong transition-colors duration-150 group-hover:bg-seal lg:block"
                  />
                  <span className="lg:max-w-[23rem] lg:flex-none">
                    <span className="mr-1.5 font-semibold text-fg">{NOTES[i].label}.</span>
                    {NOTES[i].body}
                  </span>
                </p>
              </li>
            ))}
            <li className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-x-3 px-5 pt-2 sm:px-8">
              <span />
              <code className="font-mono text-[13px] text-fg-faint sm:text-[14.5px]">
                {"{ … }"}
                <span className="ml-3 font-sans text-[13px]">
                  the body: checked against the clauses above, then out of the way
                </span>
              </code>
            </li>
          </ol>
          <figcaption className="relative flex flex-wrap items-end justify-between gap-x-10 gap-y-4 border-t border-dashed border-line-strong px-5 pt-5 pb-6 sm:px-8">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13.5px]">
              <dt className="text-fg-faint">Reviewed by</dt>
              <dd className="font-medium">a person, reading five lines</dd>
              <dt className="text-fg-faint">Held by</dt>
              <dd className="font-medium">
                <code className="font-mono text-[0.92em]">vow check</code>, on every change
              </dd>
            </dl>
            <Seal className="mr-2 ml-auto size-20 -rotate-12 sm:size-24" />
          </figcaption>
        </figure>
      </div>
    </section>
  );
}
