import type { ReactNode } from "react";
import { Seal } from "@/components/seal";
import { TokenSpans } from "@/components/token-spans";
import { tokenize } from "@/lib/sakura/highlight";
import { cn } from "@/lib/utils";
import { sample } from "./samples";
import { WINDOW, WindowBar } from "./window";

// One signature, in the formatter's multi-line form (design §12.5), read clause by clause.
const SIGNATURE = sample("withdraw");

const C = ({ children }: { children: ReactNode }) => (
  <code className="font-mono text-[0.9em] text-foreground">{children}</code>
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
    <section className="border-b border-border">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,22rem)] md:items-end md:gap-16">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            The signature is the contract.
          </h2>
          <p className="text-[17px] leading-relaxed text-pretty text-muted-foreground">
            Five lines a person can review. The compiler holds the body to every one of them.
          </p>
        </div>

        {/* The contract, on ruled paper, with the compiler's seal on the signature line. */}
        <figure className={cn(WINDOW, "relative mt-12")}>
          <WindowBar className="justify-between sm:px-4">
            <span>src/accounts.sak</span>
            <span className="hidden text-muted-foreground sm:inline">hover a clause</span>
          </WindowBar>
          <ol className="py-3 sm:py-5">
            {lines.map((line, i) => (
              <li
                key={i}
                className="group grid grid-cols-[1.75rem_minmax(0,1fr)] gap-x-3 px-5 py-2 transition-colors duration-150 hover:bg-sakura-wash/70 sm:px-8 lg:grid-cols-[1.75rem_minmax(0,1.25fr)_minmax(0,1fr)] lg:items-baseline lg:gap-x-6"
              >
                <span
                  aria-hidden
                  className="row-span-2 grid size-6 place-items-center self-start rounded-full border border-strong font-mono text-[11px] text-muted-foreground tabular-nums transition-colors duration-150 group-hover:border-sakura group-hover:bg-sakura group-hover:text-ink lg:row-span-1 lg:self-baseline"
                >
                  {i + 1}
                </span>
                <code className="pl-[4ch] -indent-[4ch] font-mono text-[12.5px] leading-relaxed [overflow-wrap:anywhere] whitespace-pre-wrap sm:pl-0 sm:indent-0 sm:text-[14.5px] sm:whitespace-pre">
                  <TokenSpans line={line} />
                </code>
                <p className="mt-1 text-[14px] leading-snug text-muted-foreground lg:mt-0 lg:flex lg:items-baseline lg:gap-3">
                  <span
                    aria-hidden
                    className="hidden h-px min-w-6 flex-1 translate-y-[-0.3em] bg-strong transition-colors duration-150 group-hover:bg-sakura lg:block"
                  />
                  <span className="lg:max-w-[23rem] lg:flex-none">
                    <span className="mr-1.5 font-semibold text-foreground">{NOTES[i].label}.</span>
                    {NOTES[i].body}
                  </span>
                </p>
              </li>
            ))}
            <li className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-x-3 px-5 pt-2 sm:px-8">
              <span />
              <code className="font-mono text-[13px] text-faint sm:text-[14.5px]">
                {"{ … }"}
                <span className="ml-3 font-sans text-[13px]">
                  the body: checked against the clauses above, then out of the way
                </span>
              </code>
            </li>
          </ol>
          <figcaption className="relative flex items-center justify-between gap-x-6 border-t border-dashed border-strong px-5 py-5 sm:items-end sm:gap-x-10 sm:px-8 sm:pb-6">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13.5px]">
              <dt className="text-faint">Reviewed by</dt>
              <dd className="font-medium">a person, reading five lines</dd>
              <dt className="text-faint">Held by</dt>
              <dd className="font-medium">
                <code className="font-mono text-[0.92em]">sakura check</code>, on every change
              </dd>
            </dl>
            <Seal className="size-16 shrink-0 -rotate-12 sm:mr-2 sm:size-24" />
          </figcaption>
        </figure>
      </div>
    </section>
  );
}
