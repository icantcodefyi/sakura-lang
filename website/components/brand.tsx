import type { SVGProps } from "react";

/**
 * The mark: a V that is also a check. The compiler checks the vow.
 * Short left arm, long right arm, one continuous solid stroke.
 */
export const MARK_PATH = "M7.5 15.5 13.6 24 25 7.5";

export function BrandMark({ tile = false, ...props }: SVGProps<SVGSVGElement> & { tile?: boolean }) {
  return (
    <svg viewBox="0 0 32 32" fill="none" aria-hidden="true" {...props}>
      {tile ? <rect width="32" height="32" rx="8" fill="var(--mark-tile, #1a1d22)" /> : null}
      <path
        d={MARK_PATH}
        stroke={tile ? "var(--seal)" : "var(--mark, var(--seal))"}
        strokeWidth={tile ? 4.6 : 5}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function Wordmark({ className }: { className?: string }) {
  return (
    <span className={`inline-flex items-center gap-2 ${className ?? ""}`}>
      <BrandMark className="size-[1.15em] -translate-y-px" />
      <span className="font-display text-[1.05em] font-semibold tracking-[-0.03em]">vow</span>
    </span>
  );
}
