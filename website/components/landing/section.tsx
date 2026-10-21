import Link from "next/link";
import type { ReactNode } from "react";

/**
 * A landing section: the reader's question as the eyebrow, the answer as the heading, then the
 * figure. Every section shares this rhythm, so the page reads as one list of answers.
 */
export function Section({
  id,
  ask,
  title,
  lede,
  children,
  className = "",
}: {
  id: string;
  ask: string;
  title: ReactNode;
  lede?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section
      id={id}
      aria-labelledby={`${id}-title`}
      className={`border-b border-border ${className}`}
    >
      <div className="mx-auto max-w-6xl px-5 py-20 sm:px-8 md:py-24">
        <div className="grid gap-x-16 gap-y-5 md:grid-cols-[minmax(0,1fr)_minmax(0,26rem)] md:items-end">
          <div>
            <p className="font-mono text-[12px] tracking-[0.14em] text-sakura-text uppercase">
              {ask}
            </p>
            <h2
              id={`${id}-title`}
              className="mt-3 font-display text-[clamp(1.9rem,3.4vw,2.7rem)] leading-[1.05] font-semibold tracking-[-0.03em] text-balance"
            >
              {title}
            </h2>
          </div>
          {lede ? (
            <p className="text-[16.5px] leading-relaxed text-pretty text-muted-foreground">
              {lede}
            </p>
          ) : null}
        </div>
        <div className="mt-12">{children}</div>
      </div>
    </section>
  );
}

/** An inline link in landing copy. */
export function TextLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <Link
      href={href}
      className="font-medium text-foreground underline decoration-sakura underline-offset-4 transition-colors duration-150 hover:text-sakura-text"
    >
      {children}
    </Link>
  );
}
