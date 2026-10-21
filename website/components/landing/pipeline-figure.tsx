import type { CSSProperties } from "react";
import { PETAL_ANGLES, SAKURA_CUTS } from "@/components/brand";
import { Play } from "./play";

// Backends and their level (docs/stability, "By area"): JavaScript and WebAssembly ship in the
// release; `--target native` is experimental.
const TARGETS = [
  { y: 44, name: "JavaScript", note: "Node 26", ships: true },
  { y: 104, name: "WebAssembly", note: "--target wasm", ships: true },
  { y: 164, name: "native", note: "--target native", ships: false },
];

// Petals in flight: which target each heads for, staggered over one loop.
const FLIGHTS = [0, 1, 0, 2, 1, 0, 1, 2];

const PETAL = SAKURA_CUTS.main.petal;

/**
 * Source through the checker to each backend, with petals carrying the build along the lines
 * (CSS `pp-*`, transform and opacity only). Petals bound for native stop halfway and fade:
 * that line is dashed because the target is experimental. Static frame: lines and labels only.
 */
export function PipelineFigure() {
  return (
    <Play>
      <svg
        viewBox="0 0 440 208"
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- an inline, animated SVG figure
        role="img"
        aria-label="A .sak source file goes through sakura check, then builds to JavaScript, which runs on Node 26, or to WebAssembly; both ship. Native code generation is experimental."
        className="w-full"
      >
        <line x1="70" y1="104" x2="150" y2="104" stroke="var(--border-strong)" />
        {TARGETS.map((t) => (
          <line
            key={t.name}
            x1="206"
            y1="104"
            x2="282"
            y2={t.y}
            stroke={t.ships ? "var(--border-strong)" : "var(--border-strong)"}
            strokeDasharray={t.ships ? undefined : "3 5"}
          />
        ))}

        {FLIGHTS.map((to, i) => (
          <g
            key={i}
            className={to === 2 ? "pp pp-stall" : "pp pp-go"}
            style={
              {
                "--pp-delay": `${(i * 5.6) / FLIGHTS.length}s`,
                "--pp-y": `${TARGETS[to].y - 104}px`,
              } as CSSProperties
            }
          >
            <path d={PETAL} fill="var(--sakura)" transform="scale(0.2) translate(-50 -22)" />
          </g>
        ))}

        <rect
          x="8"
          y="90"
          width="62"
          height="28"
          rx="6"
          fill="var(--card)"
          stroke="var(--border-strong)"
        />
        <text
          x="39"
          y="108.5"
          textAnchor="middle"
          fontSize="12.5"
          className="font-mono"
          fill="var(--foreground)"
        >
          .sak
        </text>

        <circle cx="178" cy="104" r="28" fill="var(--seal-disc)" stroke="var(--seal-line)" />
        <g transform="translate(160.5 86.8) scale(0.35)" fill="var(--sakura)">
          {PETAL_ANGLES.map((a) => (
            <path key={a} d={PETAL} transform={a ? `rotate(${a} 50 50)` : undefined} />
          ))}
          <circle cx="50" cy="50" r={SAKURA_CUTS.main.core} fill="var(--magenta)" />
        </g>
        <text
          x="178"
          y="152"
          textAnchor="middle"
          fontSize="12"
          className="font-mono"
          fill="var(--muted-foreground)"
        >
          sakura check
        </text>

        {TARGETS.map((t) => (
          <g key={t.name}>
            <circle
              cx="288"
              cy={t.y}
              r="5"
              fill={t.ships ? "var(--ok)" : "var(--background)"}
              stroke={t.ships ? "var(--ok)" : "var(--border-strong)"}
              strokeDasharray={t.ships ? undefined : "2 2"}
            />
            <text x="302" y={t.y - 2} fontSize="13.5" fontWeight="600" fill="var(--foreground)">
              {t.name}
            </text>
            <text
              x="302"
              y={t.y + 14}
              fontSize="11.5"
              className="font-mono"
              fill="var(--muted-foreground)"
            >
              {t.ships ? t.note : "experimental"}
            </text>
          </g>
        ))}
      </svg>
    </Play>
  );
}
