import "server-only";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { site } from "@/lib/site";

// The benchmarks page, read at build time from the blessed counted baselines and the evidence
// files of the same commit (docs/release.md §2). Nothing here is copied by hand: a re-bless
// changes the page (vercel.json redeploys when bench/baselines changes).

/** The reference host: the Apple M4 Pro the baselines are counted on. */
const HOST = "d88970189598";
const BENCH = join(process.cwd(), "..", "bench");

type Cell = {
  program: string;
  variant: string;
  backend?: string | null;
  engine: string;
  size: number;
  instructions: number;
  instruction_spread: number;
  process_instructions?: number[];
  median_ms: number;
  p95_ms?: number;
  budget_ms?: number | null;
  target_ms?: number | null;
  checksum_ok: boolean;
};
type Run = {
  meta: {
    commit: string;
    started: string;
    host: { id: string; cpu: string; os: string };
    engines: Record<string, string>;
    flags: { processes: number; node?: string[] };
  };
  results: Cell[];
};
type EnvelopeRow = {
  program: string;
  size: number;
  engine: string;
  against: string;
  ceiling: number;
  status: "met" | "miss" | "absent";
  gating: boolean;
};
type Envelope = { commit: string; reading: string; direction: string; rows: EnvelopeRow[] };

const read = <T>(...path: string[]): T => JSON.parse(readFileSync(join(BENCH, ...path), "utf8"));

const js = read<Run>("baselines", `${HOST}.json`);
const wasm = read<Run>("baselines", "wasm", `${HOST}.json`);
if (js.meta.commit !== wasm.meta.commit) throw new Error("the js and wasm baselines differ");
const short = js.meta.commit.slice(0, 8);
// The evidence files of the blessed commit: bench/results/<date>-<commit>-<suite>.json.
const evidence = <T>(suite: string) =>
  read<T>("results", `${js.meta.started.slice(0, 10)}-${short}-${suite}.json`);
const native = evidence<Run>("native");
const compile = evidence<Run>("compile");
const envelope = evidence<Envelope>("envelope");
for (const run of [js, wasm, native, compile])
  if (run.results.some((c) => !c.checksum_ok)) throw new Error("a counted cell failed its oracle");

// ---------- Formatting ----------

/** `1.02 G`, `682 M`, `44.3 K`: three significant figures. */
export function count(n: number) {
  const [d, u] = n >= 1e9 ? [1e9, "G"] : n >= 1e6 ? [1e6, "M"] : n >= 1e3 ? [1e3, "K"] : [1, ""];
  return `${(n / d).toPrecision(3)}${u ? ` ${u}` : ""}`;
}
export const times = (r: number) => `${r.toFixed(2)}×`;
const percent = (r: number) => `${(r * 100).toFixed(2)}%`;
const median = (xs: number[]) => {
  const s = xs.toSorted((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const grouped = (n: number) => n.toLocaleString("en-US");

// ---------- Lookups ----------

const key = (c: Pick<Cell, "program" | "variant" | "engine" | "size">) =>
  `${c.program}|${c.variant}|${c.engine}|${c.size}`;
const index = (run: Run) => new Map(run.results.map((c) => [key(c), c]));
const jsCells = index(js);
const wasmCells = index(wasm);
const nativeCells = index(native);

/** Every program of a run, in name order, at its largest size. */
const largest = (run: Run) => {
  const top = new Map<string, number>();
  for (const c of run.results) top.set(c.program, Math.max(top.get(c.program) ?? 0, c.size));
  return [...top].toSorted(([a], [b]) => a.localeCompare(b));
};

const get = (
  cells: Map<string, Cell>,
  program: string,
  variant: string,
  engine: string,
  size: number,
) => cells.get(key({ program, variant, engine, size }));

/** Sakura's count over hand JavaScript's, at one size on one engine. */
function ratio(program: string, size: number, engine: "node" | "bun") {
  const s = get(jsCells, program, "sakura", engine, size);
  const h = get(jsCells, program, "js", engine, size);
  return s && h ? s.instructions / h.instructions : undefined;
}

// ---------- Tables ----------

export type Table = {
  head: string[];
  /** `r` right-aligns a numeric column. */
  align: ("l" | "r")[];
  /** Columns whose cells are code (program names). */
  code?: number[];
  rows: string[][];
  /** Plain sentences computed from the same data, printed after the table. */
  summary?: string;
};

const DASH = "—";
const cellOr = (c: Cell | undefined) => (c ? count(c.instructions) : DASH);

function jsTable(): Table {
  return {
    head: [
      "Program",
      "n",
      "Sakura, Node",
      "hand JS, Node",
      "ratio",
      "Sakura, Bun",
      "hand JS, Bun",
      "ratio",
    ],
    align: ["l", "r", "r", "r", "r", "r", "r", "r"],
    code: [0],
    rows: largest(js).map(([p, n]) => [
      p,
      grouped(n),
      cellOr(get(jsCells, p, "sakura", "node", n)),
      cellOr(get(jsCells, p, "js", "node", n)),
      times(ratio(p, n, "node")!),
      cellOr(get(jsCells, p, "sakura", "bun", n)),
      cellOr(get(jsCells, p, "js", "bun", n)),
      times(ratio(p, n, "bun")!),
    ]),
  };
}

function wasmTable(): Table {
  const rows = largest(wasm).map(([p, n]) => {
    const node = get(wasmCells, p, "sakura", "node", n);
    const wt = get(wasmCells, p, "sakura", "wasmtime", n);
    const hand = get(wasmCells, p, "js", "node", n)!;
    return [
      p,
      grouped(n),
      cellOr(node),
      wt ? count(wt.instructions) : "absent",
      count(hand.instructions),
      node ? times(node.instructions / hand.instructions) : DASH,
      wt ? times(wt.instructions / hand.instructions) : DASH,
    ];
  });
  const absent = rows.filter((r) => r[3] === "absent").map((r) => `\`${r[0]}\``);
  return {
    head: [
      "Program",
      "n",
      "Sakura WASM, Node",
      "Sakura WASM, Wasmtime",
      "hand JS, Node",
      "ratio, Node",
      "ratio, Wasmtime",
    ],
    align: ["l", "r", "r", "r", "r", "r", "r"],
    code: [0],
    rows,
    summary: absent.length
      ? `${absent.join(", ")} has no Wasmtime cell: it needs JSON decoding, which only the Node host implements.`
      : undefined,
  };
}

const LANGUAGES = [
  ["rust", "Rust"],
  ["go", "Go"],
  ["ocaml", "OCaml"],
  ["koka", "Koka"],
  ["koka-noreuse", "Koka, no reuse"],
] as const;

function nativeTable(): Table {
  return {
    head: ["Program", "n", ...LANGUAGES.map(([, name]) => name)],
    align: ["l", "r", ...LANGUAGES.map(() => "r" as const)],
    code: [0],
    rows: largest(native).map(([p, n]) => [
      p,
      grouped(n),
      ...LANGUAGES.map(([lang]) => cellOr(get(nativeCells, p, lang, "native", n))),
    ]),
  };
}

const ms = (x?: number | null) =>
  x == null ? DASH : x < 10 ? x.toFixed(1) : Math.round(x).toString();

function compileTable(): Table {
  const rows = compile.results.map((c) => {
    const p95 = c.p95_ms ?? c.median_ms;
    const status =
      c.budget_ms != null && p95 > c.budget_ms
        ? "over budget"
        : c.target_ms != null && p95 > c.target_ms
          ? "target missed"
          : c.budget_ms != null || c.target_ms != null
            ? "within"
            : DASH;
    return [
      c.program,
      grouped(c.size),
      ms(c.median_ms),
      ms(c.p95_ms),
      ms(c.budget_ms),
      ms(c.target_ms),
      status,
      c.instructions ? count(c.instructions) : DASH,
    ];
  });
  const over = rows.filter((r) => r[6] === "over budget").length;
  const missed = rows.filter((r) => r[6] === "target missed").map((r) => `\`${r[0]}\``);
  return {
    head: ["Case", "lines", "median", "p95", "budget", "target", "verdict", "instructions"],
    align: ["l", "r", "r", "r", "r", "r", "l", "r"],
    code: [0],
    rows,
    summary: `${over ? `${over} case${over > 1 ? "s are" : " is"} over budget` : "Every case is within its budget"}${
      missed.length
        ? `; ${missed.join(" and ")} miss${missed.length > 1 ? "" : "es"} the tighter target`
        : ""
    }.`,
  };
}

/**
 * Design §17.109's JS targets: a ceiling per program on Sakura over hand JavaScript, on each
 * engine (`map_set` 5.0 on Bun, §17.152), with the median of the fifteen at most 1.5. One row
 * per family: `handled` at depth 1, `contracts` with its checks on.
 */
const CEILINGS: [program: string, node: number, bun: number][] = [
  ["fib", 1.5, 1.5],
  ["countdown", 1.3, 1.3],
  ["sieve", 1.5, 1.5],
  ["nbody", 1.3, 1.3],
  ["spectral_norm", 1.3, 1.3],
  ["binary_trees", 1.5, 1.5],
  ["pipeline", 2.0, 2.0],
  ["rbtree", 1.5, 1.5],
  ["map_set", 3.0, 5.0],
  ["shapes", 1.5, 1.5],
  ["tokenizer", 1.5, 1.5],
  ["json_validate", 2.0, 2.0],
  ["deltablue", 2.0, 2.0],
  ["handled_d1", 2.0, 2.0],
  ["contracts", 2.0, 2.0],
];
const MEDIAN_CEILING = 1.5;
const sizeOf = new Map(largest(js));

export type Target = {
  program: string;
  ceiling: { node: number; bun: number };
  node: number;
  bun: number;
};
export const targets: Target[] = CEILINGS.map(([program, node, bun]) => {
  const n = sizeOf.get(program)!;
  return {
    program,
    ceiling: { node, bun },
    node: ratio(program, n, "node")!,
    bun: ratio(program, n, "bun")!,
  };
});
const met = (e: "node" | "bun") => targets.filter((t) => t[e] <= t.ceiling[e]).length;
export const targetMedian = {
  node: median(targets.map((t) => t.node)),
  bun: median(targets.map((t) => t.bun)),
  ceiling: MEDIAN_CEILING,
};

const ceilingOf = (t: Target) =>
  t.ceiling.node === t.ceiling.bun
    ? t.ceiling.node.toFixed(1)
    : `${t.ceiling.node.toFixed(1)} (Bun ${t.ceiling.bun.toFixed(1)})`;
const verdict = (r: number, c: number) => (r <= c ? "met" : "miss");

function targetsTable(): Table {
  return {
    head: ["Program", "ceiling", "Node", "verdict", "Bun", "verdict"],
    align: ["l", "r", "r", "l", "r", "l"],
    code: [0],
    rows: targets.map((t) => [
      t.program === "handled_d1" ? "handled (depth 1)" : t.program,
      ceilingOf(t),
      times(t.node),
      verdict(t.node, t.ceiling.node),
      times(t.bun),
      verdict(t.bun, t.ceiling.bun),
    ]),
    summary: `At the largest size, ${met("node")} of the ${targets.length} programs meet their ceiling on Node and ${met("bun")} on Bun. The median ratio is ${times(targetMedian.node)} on Node and ${times(targetMedian.bun)} on Bun, against a target of at most ${MEDIAN_CEILING.toFixed(1)}×.`,
  };
}

/** The share of a cell's five pairs that agree: the spread of the middle three. */
const middleSpread = (c: Cell) => {
  const s = (c.process_instructions ?? []).toSorted((a, b) => a - b).slice(1, -1);
  return s.length ? (s[s.length - 1] - s[0]) / median(s) : 0;
};

function spreadTable(): Table {
  const groups: [string, Cell[]][] = [
    ["Node, JavaScript backend and hand JS", js.results.filter((c) => c.engine === "node")],
    ["Bun, JavaScript backend and hand JS", js.results.filter((c) => c.engine === "bun")],
    ["Node, WebAssembly backend and hand JS", wasm.results.filter((c) => c.engine === "node")],
    ["Wasmtime", wasm.results.filter((c) => c.engine === "wasmtime")],
    ["Native controls", native.results],
  ];
  const rows = groups.map(([name, cells]) => {
    const wide = cells.filter((c) => c.instruction_spread > 0.01);
    return [
      name,
      grouped(cells.length),
      percent(median(cells.map((c) => c.instruction_spread))),
      grouped(wide.length),
      grouped(wide.filter((c) => middleSpread(c) > 0.01).length),
    ];
  });
  return {
    head: ["Engine", "cells", "median spread", "cells over 1%", "middle three over 1%"],
    align: ["l", "r", "r", "r", "r"],
    rows,
  };
}

// ---------- The envelope ----------

const gating = envelope.rows.filter((r) => r.gating);
const tally = (rows: EnvelopeRow[]) => ({
  met: rows.filter((r) => r.status === "met").length,
  miss: rows.filter((r) => r.status === "miss").length,
  absent: rows.filter((r) => r.status === "absent").length,
});
export const envelopeTally = { ...tally(gating), cells: gating.length };

const AGAINST: Record<string, string> = {
  rust: "Rust",
  go: "Go",
  ocaml: "OCaml",
  koka: "Koka",
  "koka-noreuse": "Koka, no reuse",
};

function envelopeTable(): Table {
  const pairs = new Map<string, EnvelopeRow[]>();
  for (const r of gating) {
    const k = `${r.against}|${r.ceiling}`;
    pairs.set(k, [...(pairs.get(k) ?? []), r]);
  }
  const rows = [...pairs].map(([k, rs]) => {
    const [against, ceiling] = k.split("|");
    const programs = [...new Set(rs.map((r) => r.program))];
    const t = tally(rs);
    return [
      AGAINST[against] ?? against,
      `${Number(ceiling).toFixed(1)}×`,
      grouped(programs.length),
      grouped(rs.length),
      grouped(t.met),
      grouped(t.miss),
      grouped(t.absent),
    ];
  });
  const e = envelopeTally;
  return {
    head: ["Against", "ceiling", "programs", "cells", "met", "miss", "absent"],
    align: ["l", "r", "r", "r", "r", "r", "r"],
    rows,
    summary: `The envelope is a miss on ${e.miss} of ${e.cells} gating cells (${e.met} met, ${e.absent} absent), carried. The reading is ${envelope.reading}: wall times on a loaded host give the direction only.`,
  };
}

export const tables = {
  js: jsTable(),
  wasm: wasmTable(),
  native: nativeTable(),
  compile: compileTable(),
  targets: targetsTable(),
  spread: spreadTable(),
  envelope: envelopeTable(),
} satisfies Record<string, Table>;
export type TableName = keyof typeof tables;

/** A compile case's median wall time in milliseconds (for the landing page). */
export const compileMs = (program: string) => {
  const c = compile.results.find((r) => r.program === program);
  if (!c) throw new Error(`no compile case ${program}`);
  return c.median_ms;
};

// ---------- The run ----------

const day = (iso: string) => iso.slice(0, 10);
export const run = {
  commit: js.meta.commit,
  short,
  commitUrl: `${site.github}/commit/${js.meta.commit}`,
  date: day(js.meta.started),
  cpu: js.meta.host.cpu,
  os: js.meta.host.os,
  host: js.meta.host.id,
  processes: js.meta.flags.processes,
  nodeFlags: (js.meta.flags.node ?? []).join(" "),
  engines: js.meta.engines,
  wasmtime: wasm.meta.engines.wasmtime,
};

const engineList = () => {
  const e = run.engines;
  return [
    `Node ${e.node}`,
    `Bun ${e.bun}`,
    `Wasmtime ${run.wasmtime}`,
    `Rust ${e.rustc}`,
    `Go ${e.go}`,
    `OCaml ${e.ocaml}`,
    `Koka ${e.koka}`,
  ].join(", ");
};

/** The run's facts as label/value pairs: the page's header block and its Markdown. */
export const runFacts: [label: string, value: string, href?: string][] = [
  ["Commit", run.short, run.commitUrl],
  ["Counted", run.date],
  ["Machine", `${run.cpu}, ${run.os}`],
  ["Engines", engineList()],
  ["Node flags", run.nodeFlags],
  ["Processes", `${run.processes} pairs per cell, the median reported`],
];

// ---------- Markdown, for the .md and llms routes ----------

const escape = (s: string) => s.replaceAll("|", "\\|");
export function tableMarkdown(t: Table) {
  const cell = (s: string, i: number) =>
    t.code?.includes(i) && s !== DASH && !s.includes(" ") ? `\`${s}\`` : escape(s);
  const lines = [
    `| ${t.head.join(" | ")} |`,
    `|${t.align.map((a) => (a === "r" ? "---:" : "---")).join("|")}|`,
    ...t.rows.map((r) => `| ${r.map(cell).join(" | ")} |`),
  ];
  return [lines.join("\n"), t.summary].filter(Boolean).join("\n\n");
}

export const runMarkdown = () =>
  runFacts
    .map(
      ([label, value, href]) =>
        `- **${label}:** ${href ? `[\`${value}\`](${href})` : label === "Node flags" ? `\`${value}\`` : value}`,
    )
    .join("\n");

export const chartMarkdown = () =>
  `**Chart:** Sakura's instructions per call over hand JavaScript's, per program at its largest size, on Node and on Bun, with each program's ceiling. The same data is in the target table below.`;
