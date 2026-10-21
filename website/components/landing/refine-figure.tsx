import type { CSSProperties } from "react";
import { Play } from "./play";

// Three calls into a refined parameter, as `sakura check` judges them. The two failing calls are
// the checked samples refine-literal.sak (V5004) and refine-int.sak (V2001); `pay(250)` passes.
const COLUMNS = [
  { x: 70, call: "pay(250)", pass: true, code: "ok", why: "250 >= 0 holds" },
  { x: 200, call: "pay(-5)", pass: false, code: "V5004", why: "never a valid Cents" },
  { x: 330, call: "pay(n)", pass: false, code: "V2001", why: "an Int is not a Cents" },
];

const MONO = "font-mono";

/**
 * Values fall onto the refinement; a pass drops through to `Cents`, a failure stops on the clause
 * and its diagnostic appears. CSS keyframes in globals.css (`rf-*`), one column after another;
 * the static frame, and the reduced-motion one, is the end state with every result shown.
 */
export function RefineFigure() {
  return (
    <Play>
      <svg
        viewBox="0 0 400 236"
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- an inline, animated SVG figure
        role="img"
        aria-label="sakura check on three calls to pay, whose parameter is Cents, an Int where it >= 0: pay(250) compiles; pay(-5) is error V5004, the literal is never a valid Cents; pay(n) with n an Int is error V2001, an Int is not a Cents."
        className="w-full overflow-visible"
      >
        <rect x="8" y="100" width="384" height="34" rx="7" fill="var(--sakura-wash)" />
        {COLUMNS.map((c, i) => (
          <g key={c.call} style={{ "--rf-delay": `${i * 1.4}s` } as CSSProperties}>
            <rect
              x={c.x - 46}
              y="8"
              width="92"
              height="28"
              rx="6"
              fill="var(--card)"
              stroke="var(--border-strong)"
            />
            <text
              x={c.x}
              y="26.5"
              textAnchor="middle"
              fontSize="12.5"
              className={MONO}
              fill="var(--foreground)"
            >
              {c.call}
            </text>
            <line
              x1={c.x}
              y1="36"
              x2={c.x}
              y2="100"
              stroke="var(--border-strong)"
              strokeDasharray="2 4"
            />
            {c.pass ? (
              <line x1={c.x} y1="134" x2={c.x} y2="178" stroke="var(--ok)" strokeOpacity="0.6" />
            ) : (
              <rect
                className="rf-flash"
                x={c.x - 62}
                y="100"
                width="124"
                height="34"
                fill="var(--fail-wash)"
              />
            )}
            <circle
              className={c.pass ? "rf-dot rf-pass" : "rf-dot rf-stop"}
              cx={c.x}
              cy="44"
              r="5"
              fill={c.pass ? "var(--ok)" : "var(--sakura)"}
            />
            <g className={c.pass ? "rf-result rf-late" : "rf-result rf-early"}>
              <rect
                x={c.x - 40}
                y="180"
                width="80"
                height="26"
                rx="13"
                fill={c.pass ? "var(--ok-wash)" : "var(--fail-wash)"}
                stroke={c.pass ? "var(--ok)" : "var(--fail)"}
                strokeOpacity="0.45"
              />
              <text
                x={c.x}
                y="197.5"
                textAnchor="middle"
                fontSize="12"
                className={MONO}
                fill={c.pass ? "var(--ok)" : "var(--fail)"}
              >
                {c.pass ? "✓ Cents" : c.code}
              </text>
              <text
                x={c.x}
                y="226"
                textAnchor="middle"
                fontSize="11.5"
                fill="var(--muted-foreground)"
              >
                {c.why}
              </text>
            </g>
          </g>
        ))}
        {/* The clause itself, drawn over the falling dots' path. */}
        <rect x="8" y="100" width="384" height="34" rx="7" fill="none" stroke="var(--sakura)" />
        <text
          x="200"
          y="121.5"
          textAnchor="middle"
          fontSize="12.5"
          className={MONO}
          fill="var(--foreground)"
        >
          type Cents = Int <tspan fill="var(--sakura-text)">where it &gt;= 0</tspan>
        </text>
      </svg>
    </Play>
  );
}
