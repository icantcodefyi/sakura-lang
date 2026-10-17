import type { SVGProps } from "react";

/**
 * The Sakura Seal: five notched petals around a core. The gaps are cut out, not painted, so the
 * mark works on any background. Construction and usage rules: design/BRAND.md.
 *
 * Two optical cuts share one construction. `small` (≤ 32px) has wider gaps, a deeper notch and a
 * bigger core so the gaps stay ≥ 1px at 16px; `main` is for everything larger.
 * Each petal is drawn upright around (50, 50), rotated in 72° steps, then the flower is nudged
 * down so its ink box, not its centre, sits in the middle of the viewBox.
 */
export const SAKURA_CUTS = {
  main: {
    petal:
      "M55.01 39.54L59.39 33.5C64.89 25.94 67.4 21.3 67.4 16.5C67.4 8.5 64 3 59.8 3C57.4 3 50.9 7 50 9.2C49.1 7 42.6 3 40.2 3C36 3 32.6 8.5 32.6 16.5C32.6 21.3 35.11 25.94 40.61 33.5L44.99 39.54A11.6 11.6 0 0 1 55.01 39.54Z",
    core: 7.4,
    dy: 1.14,
  },
  small: {
    petal:
      "M56.41 35.56L59.36 31.5C64.21 24.83 67.8 21.3 67.8 16.5C67.8 8.5 64.2 3 60 3C57.6 3 51.1 7.8 50 10.6C48.9 7.8 42.4 3 40 3C35.8 3 32.2 8.5 32.2 16.5C32.2 21.3 35.79 24.83 40.64 31.5L43.59 35.56A15.8 15.8 0 0 1 56.41 35.56Z",
    core: 9.2,
    dy: 1.07,
  },
} as const;

export const PETAL_ANGLES = [0, 72, 144, 216, 288] as const;

export function SakuraMark({
  cut = "main",
  mono = false,
  ...props
}: SVGProps<SVGSVGElement> & { cut?: keyof typeof SAKURA_CUTS; mono?: boolean }) {
  const c = SAKURA_CUTS[cut];
  return (
    <svg viewBox="0 0 100 100" aria-hidden="true" {...props}>
      <g transform={`translate(0 ${c.dy})`}>
        <g fill={mono ? "currentColor" : "var(--sakura)"}>
          {PETAL_ANGLES.map((a) => (
            <path key={a} d={c.petal} transform={a ? `rotate(${a} 50 50)` : undefined} />
          ))}
        </g>
        <circle cx="50" cy="50" r={c.core} fill={mono ? "currentColor" : "var(--magenta)"} />
      </g>
    </svg>
  );
}

export function Wordmark({ className }: { className?: string }) {
  return (
    <span className={`inline-flex items-center gap-[0.4em] ${className ?? ""}`}>
      <SakuraMark cut="small" className="size-[1.2em]" />
      <span className="font-display text-[1.05em] font-semibold tracking-[-0.025em]">
        Sakura Lang
      </span>
    </span>
  );
}
