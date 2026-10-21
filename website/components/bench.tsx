// The benchmarks page's tables and chart, rendered on the server from lib/bench.ts. The chart is
// inline SVG with a fixed viewBox per layout (a container query picks wide or narrow, as in
// docs-diagrams.tsx), so it reserves its height and nothing shifts.
import Link from "next/link";
import { CountUp } from "@/components/count-up";
import {
  callouts,
  runFacts,
  tables,
  targets,
  targetMedian,
  times,
  type TableName,
  type Win,
} from "@/lib/bench";

export function BenchTable({ table }: { table: TableName }) {
  const t = tables[table];
  // A suite not yet blessed: its notice, or nothing if a sibling table carries it.
  if (!t.rows.length)
    return t.summary ? (
      <p className="not-prose my-6 border-y border-border py-4 font-mono text-[13px] text-faint">
        {t.summary}
      </p>
    ) : null;
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
            {t.rows.map((r, row) => (
              <tr
                key={t.ids?.[row] ?? r[0]}
                id={t.ids?.[row]}
                className={t.ids ? "scroll-mt-24 target:bg-sakura-wash" : undefined}
              >
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

// ---------- The wins ----------

/** Two measured values as bars on one linear scale from zero: Sakura in sakura, the
 * comparison in a recessive neutral. The numbers are text beside them, so the bars are
 * hidden from a screen reader. */
function WinBars({ bars }: { bars: Win["bars"] }) {
  const top = Math.max(...bars.map((b) => b.value));
  return (
    <span className="mt-5 grid grid-cols-[4rem_1fr_auto] items-center gap-x-3 gap-y-2 font-mono text-[12px]">
      {bars.map((b, i) => (
        <span key={b.label} className="contents">
          <span className="text-muted-foreground">{b.label}</span>
          <span aria-hidden className="h-1.5">
            <span
              className={`bar-grow block h-full rounded-full ${i === 0 ? "bg-[var(--chart-node)]" : "bg-muted-foreground/35"}`}
              style={{ width: `${(b.value / top) * 100}%` }}
            />
          </span>
          <span className="text-right text-foreground tabular-nums">{b.shown}</span>
        </span>
      ))}
    </span>
  );
}

/** One win: the figure, what it says, the measured pair, and a link to its row. */
export function WinCallout({
  win,
  href = "",
  count = false,
}: {
  win: Win;
  href?: string;
  /** count the figure up from zero when it scrolls into view (the landing page) */
  count?: boolean;
}) {
  return (
    <Link
      href={`${href}#${win.id}`}
      className="group flex h-full flex-col rounded-sm outline-offset-4 focus-visible:outline-2 focus-visible:outline-sakura-text"
    >
      <span className="font-display text-[clamp(2.2rem,4vw,3rem)] leading-none font-semibold tracking-[-0.03em]">
        {count ? <CountUp value={win.value} /> : win.value}
      </span>
      <span className="mt-3 text-[15px] leading-snug text-balance">{win.label}</span>
      <WinBars bars={win.bars} />
      <span className="mt-auto flex items-baseline gap-2 pt-5 font-mono text-[12px] text-faint">
        <span className="min-w-0">
          <code className="text-foreground">{win.subject}</code>, {win.note}
        </span>
        <span
          aria-hidden
          className="ml-auto text-sakura-text transition-transform duration-250 ease-[cubic-bezier(0.22,1,0.36,1)] group-hover:translate-x-1"
        >
          →
        </span>
      </span>
    </Link>
  );
}

/** The benchmarks page's wins: the landing page's callouts, two to a row. */
export function BenchWins() {
  return (
    <div className="not-prose my-8 grid border-t border-border sm:grid-cols-2">
      {callouts.map((w) => (
        <div
          key={w.id}
          className="border-b border-border py-6 sm:odd:pr-6 sm:even:border-l sm:even:pl-6"
        >
          <WinCallout win={w} />
        </div>
      ))}
    </div>
  );
}
