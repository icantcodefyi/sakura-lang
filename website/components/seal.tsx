import { useId, type CSSProperties, type SVGProps } from "react";
import { PETAL_ANGLES, SAKURA_CUTS } from "@/components/brand";

/**
 * The compiler's seal, pressed over a signature it has checked: the Sakura Seal on a blush disc,
 * with `label` running around the rim like a hanko.
 *
 * `bloom`: undefined renders it at rest; false hides it; true presses it in (the disc lands, the
 * petals open from the core one after another, the rim turns into place). Transform and opacity
 * only, and the global reduced-motion rule turns it into an instant appearance. See globals.css.
 */
export function Seal({
  label = "signature checked · promise kept · ",
  rim = true,
  bloom,
  className,
  ...props
}: SVGProps<SVGSVGElement> & { label?: string; rim?: boolean; bloom?: boolean }) {
  const id = `seal-rim-${useId()}`;
  const c = SAKURA_CUTS.main;
  const state = bloom === undefined ? "" : bloom ? "seal-bloom" : "seal-hidden";
  return (
    <svg
      viewBox="0 0 100 100"
      aria-hidden="true"
      className={`${state} ${className ?? ""}`}
      {...props}
    >
      <defs>
        <path id={id} d="M50 50m-38.5 0a38.5 38.5 0 1 1 77 0a38.5 38.5 0 1 1-77 0" />
      </defs>
      <circle cx="50" cy="50" r="49" fill="var(--seal-disc)" />
      <circle
        cx="50"
        cy="50"
        r="46.5"
        fill="none"
        stroke="var(--seal-line)"
        strokeWidth="0.8"
        strokeDasharray={rim ? undefined : "1.2 2.2"}
      />
      {rim ? (
        <g className="seal-rim">
          <circle
            cx="50"
            cy="50"
            r="31.5"
            fill="none"
            stroke="var(--seal-line)"
            strokeWidth="0.6"
          />
          <text
            fill="var(--seal-ink)"
            fontSize="7"
            letterSpacing="1.9"
            className="font-mono uppercase"
          >
            <textPath href={`#${id}`}>{label}</textPath>
          </text>
        </g>
      ) : null}
      <g
        transform={`translate(${rim ? 22.4 : 14} ${(rim ? 22.4 : 14) + c.dy * (rim ? 0.552 : 0.72)}) scale(${rim ? 0.552 : 0.72})`}
      >
        <g fill="var(--sakura)">
          {PETAL_ANGLES.map((a, k) => (
            <g key={a} transform={a ? `rotate(${a} 50 50)` : undefined}>
              <path className="seal-petal" style={{ "--k": k } as CSSProperties} d={c.petal} />
            </g>
          ))}
        </g>
        <circle className="seal-core" cx="50" cy="50" r={c.core} fill="var(--magenta)" />
      </g>
    </svg>
  );
}
