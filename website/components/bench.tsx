// The benchmarks page's tables and chart, rendered on the server from lib/bench.ts. The chart is
// inline SVG with a fixed viewBox per layout (a container query picks wide or narrow, as in
// docs-diagrams.tsx), so it reserves its height and nothing shifts.
import { runFacts, tables, targets, targetMedian, times, type TableName } from "@/lib/bench";

export function BenchTable({ table }: { table: TableName }) {
  const t = tables[table];
  return (
    <>
      {/* Focusable so a keyboard can scroll a wide table, as Fumadocs' own tables are. */}
      {/* oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex */}
      <div className="relative my-6 overflow-auto prose-no-margin" tabIndex={0}>
        <table>
          <thead>
            <tr>
              {t.head.map((h, i) => (
                <th key={i} className={t.align[i] === "r" ? "text-right" : undefined}>
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {t.rows.map((r) => (
              <tr key={r[0]}>
                {r.map((c, i) => (
                  <td
                    key={i}
                    className={
                      t.align[i] === "r"
                        ? "text-right tabular-nums"
                        : c === "miss" || c === "over budget" || c === "target missed"
                          ? "whitespace-nowrap text-fail"
                          : c === "met" || c === "within"
                            ? "whitespace-nowrap text-ok"
                            : undefined
                    }
                  >
                    {t.code?.includes(i) && !c.includes(" ") ? <code>{c}</code> : c}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {t.summary && <p>{inlineCode(t.summary)}</p>}
    </>
  );
}

/** `name` spans of a computed sentence as code. */
const inlineCode = (s: string) =>
  s
    .split(/(`[^`]+`)/)
    .map((part, i) => (part.startsWith("`") ? <code key={i}>{part.slice(1, -1)}</code> : part));

/** The counted run: commit, date, machine, engines and flags, as a flat definition list. */
export function BenchRun() {
  return (
    <dl className="not-prose my-6 grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 border-y border-border py-4 text-[14px] leading-relaxed">
      {runFacts.map(([label, value, href]) => (
        <div key={label} className="contents">
          <dt className="font-mono text-[12.5px] text-muted-foreground">{label}</dt>
          <dd className="min-w-0">
            {href ? (
              <a
                href={href}
                className="font-mono underline decoration-sakura underline-offset-[3px] hover:text-sakura-text"
              >
                {value}
              </a>
            ) : label === "Node flags" ? (
              <code className="font-mono text-[13px] break-words">{value}</code>
            ) : (
              value
            )}
          </dd>
        </div>
      ))}
    </dl>
  );
}

// ---------- The chart ----------

const LAYOUT = {
  wide: { width: 720, label: 128, right: 24, font: 12.5 },
  narrow: { width: 360, label: 104, right: 14, font: 11 },
} as const;
const TOP = 34;
const ROW = 26;
const AXIS = 28;

// A log scale over the data, with ceilings and 1× inside it.
const values = targets.flatMap((t) => [t.node, t.bun, t.ceiling.node, t.ceiling.bun, 1]);
const lo = Math.log2(Math.min(...values)) - 0.25;
const hi = Math.log2(Math.max(...values)) + 0.25;
const TICKS = [0.125, 0.25, 0.5, 1, 2, 4, 8, 16, 32].filter(
  (v) => Math.log2(v) >= lo && Math.log2(v) <= hi,
);
const tickLabel = (v: number) => (v < 1 ? `${v}×`.replace(/^0/, "") : `${v}×`);

function Chart({ which, className }: { which: keyof typeof LAYOUT; className: string }) {
  const l = LAYOUT[which];
  const plot = l.width - l.label - l.right;
  const x = (v: number) => l.label + ((Math.log2(v) - lo) / (hi - lo)) * plot;
  const height = TOP + targets.length * ROW + AXIS;
  const bottom = TOP + targets.length * ROW;
  const id = `bench-chart-${which}`;
  return (
    <svg
      viewBox={`0 0 ${l.width} ${height}`}
      width={l.width}
      height={height}
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- inline SVG keeps the theme colours and real text; an <img> can't
      role="img"
      aria-labelledby={`${id}-title ${id}-desc`}
      className={`h-auto w-full overflow-visible ${className}`}
      style={{ fontSize: l.font }}
    >
      <title id={`${id}-title`}>Sakura over hand-written JavaScript, instructions per call</title>
      <desc id={`${id}-desc`}>
        {targets
          .map(
            (t) =>
              `${t.program}: Node ${times(t.node)}, Bun ${times(t.bun)}, ceiling ${t.ceiling.node}.`,
          )
          .join(" ")}
      </desc>
      {/* Legend */}
      <g transform={`translate(${l.label} 12)`} className="fill-muted-foreground">
        <circle cx={5} cy={0} r={4.5} className="fill-[var(--chart-node)]" />
        <text x={15} y={4}>
          Node
        </text>
        <rect x={60} y={-4.5} width={9} height={9} rx={1.5} className="fill-[var(--chart-bun)]" />
        <text x={75} y={4}>
          Bun
        </text>
        <line x1={118} x2={118} y1={-6} y2={6} className="stroke-foreground" strokeWidth={2} />
        <text x={126} y={4}>
          ceiling
        </text>
      </g>
      {/* Grid */}
      {TICKS.map((v) => (
        <g key={v}>
          <line
            x1={x(v)}
            x2={x(v)}
            y1={TOP - 6}
            y2={bottom}
            className={v === 1 ? "stroke-strong" : "stroke-border"}
            strokeWidth={v === 1 ? 1.5 : 1}
          />
          <text
            x={x(v)}
            y={bottom + 18}
            textAnchor="middle"
            className="fill-muted-foreground tabular-nums"
          >
            {tickLabel(v)}
          </text>
        </g>
      ))}
      {targets.map((t, i) => {
        const y = TOP + i * ROW + ROW / 2;
        return (
          <g key={t.program}>
            <text x={l.label - 10} y={y + 4} textAnchor="end" className="fill-foreground font-mono">
              {t.program === "handled_d1" ? "handled" : t.program}
            </text>
            <line
              x1={l.label}
              x2={l.width - l.right}
              y1={y}
              y2={y}
              className="stroke-border"
              strokeDasharray="1 3"
            />
            {[t.ceiling.node, ...(t.ceiling.bun !== t.ceiling.node ? [t.ceiling.bun] : [])].map(
              (c) => (
                <line
                  key={c}
                  x1={x(c)}
                  x2={x(c)}
                  y1={y - 7}
                  y2={y + 7}
                  className="stroke-foreground"
                  strokeWidth={2}
                >
                  <title>{`${t.program} ceiling ${c}×${t.ceiling.bun !== t.ceiling.node ? (c === t.ceiling.bun ? " on Bun" : " on Node") : ""}`}</title>
                </line>
              ),
            )}
            <rect
              x={x(t.bun) - 4.5}
              y={y - 4.5}
              width={9}
              height={9}
              rx={1.5}
              className="fill-[var(--chart-bun)] stroke-background"
              strokeWidth={2}
              paintOrder="stroke"
            >
              <title>{`${t.program} on Bun: ${times(t.bun)}`}</title>
            </rect>
            <circle
              cx={x(t.node)}
              cy={y}
              r={4.5}
              className="fill-[var(--chart-node)] stroke-background"
              strokeWidth={2}
              paintOrder="stroke"
            >
              <title>{`${t.program} on Node: ${times(t.node)}`}</title>
            </circle>
          </g>
        );
      })}
    </svg>
  );
}

export function BenchChart() {
  return (
    <figure data-diagram className="not-prose @container my-8 grid gap-3">
      <Chart which="wide" className="hidden max-w-[720px] @min-[560px]:block" />
      <Chart which="narrow" className="max-w-[420px] @min-[560px]:hidden" />
      <figcaption className="border-t border-border pt-2.5 text-[13px] text-faint">
        Sakura's instructions per call over hand-written JavaScript's on the same engine, at each
        program's largest size, on a log scale. Left of 1× Sakura retires fewer instructions; a mark
        right of its ceiling misses the design target. Median: {times(targetMedian.node)} on Node,{" "}
        {times(targetMedian.bun)} on Bun, against {targetMedian.ceiling.toFixed(1)}×.
      </figcaption>
    </figure>
  );
}
