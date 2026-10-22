import "server-only";
import { existsSync, readFileSync } from "node:fs";
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
  process_counters?: { one: { rss_bytes: number } }[];
  median_ms: number;
  process_medians_ms?: number[];
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
const native = read<Run>("baselines", "native", `${HOST}.json`);
if (native.meta.commit !== js.meta.commit) throw new Error("the js and native baselines differ");
// Ports counted after the bless, on the same host and toolchains: C, and OCaml and Koka ports of
// the programs that had none. A port's count does not depend on Sakura's compiler, so its cells
// stand beside the blessed Sakura cells until the next bless takes them in (docs/release.md §2).
const NATIVE_PORTS = "2026-10-22-49c49e2a-native-ports.json";
const nativePorts = read<Run>("results", NATIVE_PORTS);
if (nativePorts.meta.host.id !== HOST) throw new Error("the native ports were counted elsewhere");
// Wall time of the same programs on a GitHub-hosted runner (bench.yml, mode native): the time
// anyone can reproduce. Its run id is in the file name; none until the first run finishes.
const HOSTED = undefined as string | undefined;
const hosted = HOSTED ? read<Run>("results", HOSTED) : undefined;
const compile = evidence<Run>("compile");
const envelope = evidence<Envelope>("envelope");
for (const run of [js, wasm, native, nativePorts, ...(hosted ? [hosted] : []), compile])
  if (run.results.some((c) => !c.checksum_ok)) throw new Error("a counted cell failed its oracle");

// ---------- Formatting ----------

/** `1.02 G`, `682 M`, `44.3 K`: three significant figures. */
export function count(n: number) {
  const [d, u] = n >= 1e9 ? [1e9, "G"] : n >= 1e6 ? [1e6, "M"] : n >= 1e3 ? [1e3, "K"] : [1, ""];
  return `${(n / d).toPrecision(3)}${u ? ` ${u}` : ""}`;
}
export const times = (r: number) => `${r.toFixed(2)}×`;
/** A native ratio: `1.84×`, `27×` (two places only where they matter). */
export const factor = (r: number) =>
  r >= 10 ? `${Math.round(r).toLocaleString("en-US")}×` : times(r);
const percent = (r: number) => `${(r * 100).toFixed(2)}%`;
const median = (xs: number[]) => {
  const s = xs.toSorted((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const day = (iso: string) => iso.slice(0, 10);
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
  /** Each row's anchor, for links from the wins band and the landing page. */
  ids?: string[];
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
    ids: compile.results.map((c) => `compile-${c.program.replaceAll("_", "-")}`),
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
    head: ["Program", "goal", "Node", "verdict", "Bun", "verdict"],
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
    summary: `At the largest size, ${met("node")} of the ${targets.length} programs meet their goal on Node and ${met("bun")} on Bun. The typical (median) program does ${times(targetMedian.node)} the work of hand-written JavaScript on Node and ${times(targetMedian.bun)} on Bun; the goal is at most ${MEDIAN_CEILING.toFixed(1)}×.`,
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

// ---------- Wins ----------
// Every cell of the same files where Sakura is ahead of its comparison, found by rule, not
// picked: a re-bless that loses a win drops it here and on the landing page.

const MIB = 2 ** 20;
const mib = (b: number) => `${(b / MIB).toFixed(1)} MiB`;
/** A cell's peak RSS over its one-call processes: one call each side, whatever `calls` was. */
const oneCallRss = (c: Cell) => Math.max(...(c.process_counters ?? []).map((p) => p.one.rss_bytes));
/** Below this a memory ratio is within run-to-run noise and not called a win. */
const MEMORY_MARGIN = 0.9;

const sizesOf = (run: Run, program: string) =>
  [...new Set(run.results.filter((c) => c.program === program).map((c) => c.size))].toSorted(
    (a, b) => a - b,
  );

export type Win = {
  /** The row's anchor on the benchmarks page. */
  id: string;
  /** `3.46×` */
  value: string;
  /** What the value says, completing a sentence after it. */
  label: string;
  subject: string;
  /** The measured pair, Sakura first: the callout's two bars. */
  bars: [sakura: Bar, other: Bar];
  note: string;
};
export type Bar = { label: string; value: number; shown: string };

type JsWin = {
  program: string;
  engine: "node" | "bun";
  n: number;
  sakura: Cell;
  hand: Cell;
  ratio: number;
  everySize: boolean;
  wallAgrees: boolean;
};
const ENGINE = { node: "Node", bun: "Bun" } as const;

const jsWins: JsWin[] = largest(js)
  .flatMap(([program, n]) =>
    (["node", "bun"] as const).map((engine) => {
      const sakura = get(jsCells, program, "sakura", engine, n)!;
      const hand = get(jsCells, program, "js", engine, n)!;
      return {
        program,
        engine,
        n,
        sakura,
        hand,
        ratio: sakura.instructions / hand.instructions,
        everySize: sizesOf(js, program).every((s) => (ratio(program, s, engine) ?? 2) < 1),
        wallAgrees: sakura.median_ms < hand.median_ms,
      };
    }),
  )
  .filter((w) => w.ratio < 1)
  .toSorted((a, b) => (a.engine === b.engine ? a.ratio - b.ratio : a.engine === "node" ? -1 : 1));

const jsWinId = (w: Pick<JsWin, "program" | "engine">) =>
  `win-${w.program.replaceAll(/[^a-z0-9]+/g, "-")}-${w.engine}`;

function winsJsTable(): Table {
  return {
    head: ["Program", "engine", "n", "Sakura", "hand JS", "ratio", "every size", "wall time"],
    align: ["l", "l", "r", "r", "r", "r", "l", "l"],
    code: [0],
    ids: jsWins.map(jsWinId),
    rows: jsWins.map((w) => [
      w.program,
      w.engine === "bun" ? "Bun (advisory)" : "Node",
      grouped(w.n),
      count(w.sakura.instructions),
      count(w.hand.instructions),
      times(w.ratio),
      w.everySize ? "yes" : "no",
      w.wallAgrees ? "faster" : "slower",
    ]),
    summary: `${jsWins.filter((w) => w.engine === "node").length} programs run fewer instructions per call than the same program written by hand in JavaScript on Node at the largest size, and ${jsWins.filter((w) => w.engine === "bun").length} on Bun. The last column is the wall time of the same cells, on a busy machine: it shows the direction only, and says so where it disagrees with the count.`,
  };
}

type MemoryWin = {
  program: string;
  backend: "js" | "wasm";
  n: number;
  sakura: number;
  hand: number;
};
const memoryWins: MemoryWin[] = [
  ...largest(js).map(([program, n]) => ({
    program,
    backend: "js" as const,
    n,
    sakura: oneCallRss(get(jsCells, program, "sakura", "node", n)!),
    hand: oneCallRss(get(jsCells, program, "js", "node", n)!),
  })),
  ...largest(wasm).map(([program, n]) => ({
    program,
    backend: "wasm" as const,
    n,
    sakura: oneCallRss(get(wasmCells, program, "sakura", "node", n)!),
    hand: oneCallRss(get(wasmCells, program, "js", "node", n)!),
  })),
]
  .filter((w) => w.sakura / w.hand < MEMORY_MARGIN)
  .toSorted((a, b) => a.sakura / a.hand - b.sakura / b.hand);
const memoryWinId = (w: MemoryWin) =>
  `memory-${w.program.replaceAll(/[^a-z0-9]+/g, "-")}-${w.backend}`;

function winsMemoryTable(): Table {
  return {
    head: ["Program", "backend", "n", "Sakura", "hand JS", "ratio"],
    align: ["l", "l", "r", "r", "r", "r"],
    code: [0],
    ids: memoryWins.map(memoryWinId),
    rows: memoryWins.map((w) => [
      w.program,
      w.backend === "wasm" ? "WebAssembly" : "JavaScript",
      grouped(w.n),
      mib(w.sakura),
      mib(w.hand),
      times(w.sakura / w.hand),
    ]),
    summary: `Peak resident memory of a fresh Node process that makes one call, the highest of the cell's five, against the same for hand JavaScript; listed when at least 10% lower. A WebAssembly program keeps its data in linear memory rather than in JavaScript objects; the same cells retire many more instructions (see the WebAssembly table).`,
  };
}

type CompileWin = {
  case: string;
  lines: number;
  p95: number;
  budget: number;
  target?: number | null;
};
const compileWins: CompileWin[] = compile.results
  .filter((c) => c.budget_ms != null && (c.p95_ms ?? c.median_ms) <= c.budget_ms)
  .map((c) => ({
    case: c.program,
    lines: c.size,
    p95: c.p95_ms ?? c.median_ms,
    budget: c.budget_ms!,
    target: c.target_ms,
  }))
  .toSorted((a, b) => b.budget / b.p95 - a.budget / a.p95);
const targetMissed = (w: CompileWin) => w.target != null && w.p95 > w.target;

// ---------- Sakura beside TypeScript ----------
// `bench typescript` (bench/README.md): blessed from v1.0.0-beta.3 on. Shown only when the
// blessed file is of the same commit as the JavaScript baseline; until then the page says so.

const tsPath = join(BENCH, "baselines", "typescript", `${HOST}.json`);
const tsRun = existsSync(tsPath) ? read<Run>("baselines", "typescript", `${HOST}.json`) : undefined;
export const typescript = tsRun?.meta.commit === js.meta.commit ? tsRun : undefined;
if (typescript?.results.some((c) => !c.checksum_ok)) throw new Error("a TypeScript cell failed");
const tsPrograms = typescript
  ? [...new Set(typescript.results.filter((c) => c.engine === "check").map((c) => c.program))]
  : [];
export const TYPESCRIPT_UNAVAILABLE =
  "TypeScript measurements are shown once a baseline matching the JavaScript run is blessed.";

function typescriptTable(): Table {
  const v = (p: string, variant: string, engine: string) =>
    typescript?.results.find(
      (c) => c.program === p && c.variant === variant && c.engine === engine,
    );
  return {
    head: ["Program", "lines", "sakura check", "uncached", "tsc --noEmit", "tsc over Sakura"],
    align: ["l", "r", "r", "r", "r", "r"],
    code: [0],
    ids: tsPrograms.map((p) => `typescript-${p.replaceAll(/[^a-z0-9]+/g, "-")}`),
    rows: tsPrograms.map((p) => {
      const s = v(p, "sakura", "check")!;
      const u = v(p, "sakura_uncached", "check");
      const t = v(p, "tsc", "check")!;
      return [
        p,
        grouped(s.size),
        count(s.instructions),
        cellOr(u),
        count(t.instructions),
        times(t.instructions / s.instructions),
      ];
    }),
    summary: typescript
      ? `Instructions per process of a clean check, the median of ${typescript.meta.flags.processes} fresh processes. TypeScript ${typescript.meta.engines.typescript}'s native compiler checks on several threads, and the count is their sum.`
      : TYPESCRIPT_UNAVAILABLE,
  };
}

function typescriptStartTable(): Table {
  const programs = typescript
    ? [...new Set(typescript.results.filter((c) => c.engine === "start").map((c) => c.program))]
    : [];
  const v = (p: string, variant: string) =>
    typescript!.results.find(
      (c) => c.program === p && c.variant === variant && c.engine === "start",
    );
  return {
    head: ["Program", "Sakura, built", "Node, main.ts", "ratio", "esbuild bundle", "ratio"],
    align: ["l", "r", "r", "r", "r", "r"],
    code: [0],
    rows: programs.map((p) => {
      const s = v(p, "sakura")!;
      const n = v(p, "node_ts")!;
      const e = v(p, "esbuild")!;
      return [
        p,
        count(s.instructions),
        count(n.instructions),
        times(s.instructions / n.instructions),
        count(e.instructions),
        times(s.instructions / e.instructions),
      ];
    }),
    summary: typescript
      ? "Instructions of a fresh Node process that loads the program, prints run(n) for n from 0 to 3 and exits: Sakura's build output, the TypeScript version run by Node directly (Node strips the types), and the same version bundled first by esbuild. A ratio is Sakura's count over the other's."
      : undefined, // the check table above carries the notice
  };
}

// ---------- The callouts ----------

/** What each program does, in a few plain words, for the callouts. */
const ABOUT: Record<string, string> = {
  fib: "naive recursive Fibonacci",
  countdown: "deep recursion",
  tail_clique: "a state machine of tail calls",
  sieve: "a prime sieve",
  nbody: "a planet simulation",
  spectral_norm: "matrix arithmetic",
  binary_trees: "building and walking trees",
  pipeline: "map, filter and sum over a list",
  rbtree: "a persistent red-black tree",
  map_set: "building and querying maps",
  shapes: "updating many small records",
  tokenizer: "splitting text into words",
  json_validate: "decoding and checking JSON",
  deltablue: "a constraint solver",
  handled_reentry: "an effect used inside a callback",
  handled_suspend: "an effect loop that sleeps once",
  contracts: "a loop with contract checks",
  contracts_plain: "a loop without contract checks",
};
const about = (program: string) =>
  ABOUT[program] ??
  ABOUT[program.split(":")[0]] ??
  (program.startsWith("handled") ? "an effect handler in a loop" : program);

const jsCallout = (w: JsWin): Win => ({
  id: jsWinId(w),
  value: times(1 / w.ratio),
  label: "less work than the same program in hand-written JavaScript",
  subject: w.program,
  bars: [
    { label: "Sakura", value: w.sakura.instructions, shown: count(w.sakura.instructions) },
    { label: "JavaScript", value: w.hand.instructions, shown: count(w.hand.instructions) },
  ],
  note: `${about(w.program)}, on ${ENGINE[w.engine]}`,
});

const memoryCallout = (w: MemoryWin): Win => ({
  id: memoryWinId(w),
  value: times(w.hand / w.sakura),
  label: "less memory than the same program in hand-written JavaScript",
  subject: w.program,
  bars: [
    { label: "Sakura", value: w.sakura, shown: mib(w.sakura) },
    { label: "JavaScript", value: w.hand, shown: mib(w.hand) },
  ],
  note: `${about(w.program)}, built to ${w.backend === "wasm" ? "WebAssembly" : "JavaScript"}`,
});

const compileCallout = (w: CompileWin): Win => {
  const build = w.case.startsWith("build");
  return {
    id: `compile-${w.case.replaceAll("_", "-")}`,
    value: `${ms(w.p95)} ms`,
    label: `to ${build ? "build" : "type-check"} a ${grouped(w.lines)}-line project, in 19 of 20 runs`,
    subject: build ? "sakura build" : "sakura check",
    bars: [
      { label: "Sakura", value: w.p95, shown: `${ms(w.p95)} ms` },
      { label: "budget", value: w.budget, shown: `${ms(w.budget)} ms` },
    ],
    note: `${times(w.budget / w.p95)} under its budget`,
  };
};

/** `sakura check` against `tsc --noEmit` on the generated 10,000-line package, once blessed,
 * with the 100,000-line package beside it, whichever way it goes. */
const tsCallout = (): Win[] => {
  const cell = (program: string, variant: string) =>
    typescript?.results.find(
      (c) => c.program === program && c.variant === variant && c.engine === "check",
    );
  const s = cell("corpus_10k", "sakura");
  const t = cell("corpus_10k", "tsc");
  if (!s || !t || t.instructions <= s.instructions) return [];
  const bs = cell("corpus_100k", "sakura");
  const bt = cell("corpus_100k", "tsc");
  const r = bs && bt ? bt.instructions / bs.instructions : undefined;
  const larger =
    !bs || r === undefined
      ? ""
      : r >= 1.05
        ? `; ${times(r)} at ${grouped(bs.size)} lines`
        : r > 0.95
          ? `; about even at ${grouped(bs.size)} lines`
          : `; tsc does ${times(1 / r)} less at ${grouped(bs.size)} lines`;
  return [
    {
      id: "typescript-corpus-10k",
      value: times(t.instructions / s.instructions),
      label: `less work than tsc to type-check a ${grouped(s.size)}-line project`,
      subject: "sakura check",
      bars: [
        { label: "Sakura", value: s.instructions, shown: count(s.instructions) },
        { label: "tsc", value: t.instructions, shown: count(t.instructions) },
      ],
      note: `TypeScript ${typescript!.meta.engines.typescript}${larger}`,
    },
  ];
};

/** The landing page's band, best first: instruction wins whose wall time agrees (one of them
 * yields to the TypeScript check once that is blessed), the compile case with the most
 * headroom and no missed target, and the largest memory win. */
const fast = jsWins.filter((w) => w.engine === "node" && w.wallAgrees).map(jsCallout);
export const callouts: Win[] = [
  ...[...fast.slice(0, 1), ...tsCallout(), ...fast.slice(1)].slice(0, 2),
  ...compileWins
    .filter((w) => !targetMissed(w))
    .slice(0, 1)
    .map(compileCallout),
  ...memoryWins.slice(0, 1).map(memoryCallout),
];

/** The other side of the wins: the median of the fifteen target programs on Node. */
export const typical: Win = {
  id: "against-hand-written-javascript",
  value: times(targetMedian.node),
  label: "the work of hand-written JavaScript, for the typical program",
  subject: "",
  bars: [
    { label: "Sakura", value: targetMedian.node, shown: times(targetMedian.node) },
    { label: "JavaScript", value: 1, shown: times(1) },
  ],
  note: `the median of ${targets.length} programs on Node; ${met("node")} meet their goal`,
};

/** The page's answer in four plain sentences, each a bold lead and the numbers behind it. */
const nodeWins = jsWins.filter((w) => w.engine === "node");
const wasmMemory = memoryWins.filter((w) => w.backend === "wasm");
const check = tsCallout()[0];
const build = compileWins.find((w) => w.case.startsWith("build") && !targetMissed(w));
// ---------- Native ----------
// Sakura's `--target native` executable beside the same programs in C, Rust, Go, OCaml and
// Koka, counted in one run on the reference Mac (Rust with overflow checks, as Sakura has them).

export const NATIVE_LANGUAGES = [
  ["c", "C"],
  ["rust", "Rust"],
  ["go", "Go"],
  ["ocaml", "OCaml"],
  ["koka", "Koka"],
] as const;
type Language = (typeof NATIVE_LANGUAGES)[number][0];
const portCells = index(nativePorts);
/** A port's cell: the blessed one, else the one counted after the bless. */
const port = (program: string, lang: string, size: number) =>
  get(nativeCells, program, lang, "native", size) ?? get(portCells, program, lang, "native", size);
const sinceBless = (program: string, lang: string, size: number) =>
  !get(nativeCells, program, lang, "native", size) &&
  !!get(portCells, program, lang, "native", size);

export type NativeRow = { lang: string; name: string; instructions: number; ratio: number };
export type NativeProgram = {
  program: string;
  about: string;
  n: number;
  sakura: number;
  rows: NativeRow[];
};
/** Every program at its largest size: Sakura's count, then each language's and Sakura ÷ it. */
export const nativePrograms: NativeProgram[] = largest(native).map(([program, n]) => {
  const sakura = get(nativeCells, program, "sakura", "native", n)!.instructions;
  return {
    program,
    about: about(program),
    n,
    sakura,
    rows: NATIVE_LANGUAGES.flatMap(([lang, name]) => {
      const c = port(program, lang, n);
      return c
        ? [{ lang, name, instructions: c.instructions, ratio: sakura / c.instructions }]
        : [];
    }),
  };
});

const geomean = (xs: number[]) => Math.exp(xs.reduce((a, x) => a + Math.log(x), 0) / xs.length);
/** One program per family (`handled` at depth 1), as the JS targets, so no family counts six times. */
const FAMILIES = new Set(CEILINGS.map(([p]) => p));

export type NativeSummaryRow = {
  lang: Language;
  name: string;
  /** Families compared, and the geometric mean of Sakura ÷ the language over them. */
  families: number;
  typical: number;
  closest: { program: string; ratio: number };
  furthest: { program: string; ratio: number };
  /** Programs (of all) where Sakura runs fewer instructions. */
  ahead: string[];
};
export const nativeSummary: NativeSummaryRow[] = NATIVE_LANGUAGES.map(([lang, name]) => {
  const all = nativePrograms.flatMap((p) =>
    p.rows.filter((r) => r.lang === lang).map((r) => ({ program: p.program, ratio: r.ratio })),
  );
  const sorted = all.toSorted((a, b) => a.ratio - b.ratio);
  const families = all.filter((r) => FAMILIES.has(r.program));
  return {
    lang,
    name,
    families: families.length,
    typical: geomean(families.map((r) => r.ratio)),
    closest: sorted[0],
    furthest: sorted[sorted.length - 1],
    ahead: sorted.filter((r) => r.ratio < 1).map((r) => r.program),
  };
}).filter((r) => r.families > 0);

/** `1.64× less work than Koka on map_set`, `within 1.84× of Rust on binary_trees`. */
export const nativeHeadline = (r: NativeSummaryRow["closest"], name: string) =>
  r.ratio < 1 / 1.05
    ? `${factor(1 / r.ratio)} less work than ${name} on \`${r.program}\``
    : r.ratio <= 1.05
      ? `about even with ${name} on \`${r.program}\``
      : `within ${factor(r.ratio)} of ${name} on \`${r.program}\``;

function nativeTable(): Table {
  return {
    head: ["Program", "n", "Sakura", ...NATIVE_LANGUAGES.map(([, name]) => name), "Koka, no reuse"],
    align: ["l", "r", "r", ...NATIVE_LANGUAGES.map(() => "r" as const), "r"],
    code: [0],
    ids: nativePrograms.map((p) => `native-${p.program.replaceAll("_", "-")}`),
    rows: nativePrograms.map(({ program: p, n, sakura }) => [
      p,
      grouped(n),
      count(sakura),
      ...[...NATIVE_LANGUAGES.map(([lang]) => lang), "koka-noreuse"].map((lang) => {
        const c = port(p, lang, n);
        return c ? `${count(c.instructions)}${sinceBless(p, lang, n) ? "*" : ""}` : DASH;
      }),
    ]),
    summary: `Instructions per call at the largest size. A dash means the program has no version in that language. ${
      nativePorts.results.some((c) => sinceBless(c.program, c.variant, c.size))
        ? `* counted on ${day(nativePorts.meta.started)} at \`${nativePorts.meta.commit.slice(0, 8)}\` on the same machine, after the baseline was blessed; these ports join the baseline at the next release.`
        : ""
    }`,
  };
}

function nativeSummaryTable(): Table {
  return {
    head: ["Against", "typical", "closest", "furthest", "Sakura ahead"],
    align: ["l", "r", "l", "l", "r"],
    rows: nativeSummary.map((r) => [
      r.name,
      factor(r.typical),
      `${factor(r.closest.ratio)} on ${r.closest.program}`,
      `${factor(r.furthest.ratio)} on ${r.furthest.program}`,
      grouped(r.ahead.length),
    ]),
    summary:
      "Sakura's instructions per call over the other language's, at each program's largest size. Typical is the geometric mean over the program families (one handled program, not six); 2× means Sakura does twice the work.",
  };
}

// ---------- Native wall time, on a GitHub-hosted runner ----------

const hostedCells = hosted ? index(hosted) : new Map<string, Cell>();
const spreadOf = (c: Cell) => {
  const xs = c.process_medians_ms ?? [];
  return xs.length ? (Math.max(...xs) - Math.min(...xs)) / median(xs) : 0;
};
const msWithSpread = (c?: Cell) =>
  c?.median_ms == null ? DASH : `${ms(c.median_ms)} ±${Math.round((spreadOf(c) * 100) / 2)}%`;
export const hostedRun = hosted && {
  id: HOSTED!.match(/hosted-(\d+)/)![1],
  url: `${site.github}/actions/runs/${HOSTED!.match(/hosted-(\d+)/)![1]}`,
  commit: hosted.meta.commit.slice(0, 8),
  commitUrl: `${site.github}/commit/${hosted.meta.commit}`,
  date: day(hosted.meta.started),
  cpu: hosted.meta.host.cpu,
  os: hosted.meta.host.os,
  processes: hosted.meta.flags.processes,
  engines: hosted.meta.engines,
};

export const hostedTypical = (hosted ? NATIVE_LANGUAGES : [])
  .map(([lang, name]) => {
    const ratios = largest(hosted!).flatMap(([p, n]) => {
      const s = get(hostedCells, p, "sakura", "native", n);
      const o = get(hostedCells, p, lang, "native", n);
      return FAMILIES.has(p) && s?.median_ms && o?.median_ms ? [s.median_ms / o.median_ms] : [];
    });
    return { name, families: ratios.length, typical: ratios.length ? geomean(ratios) : undefined };
  })
  .filter((r) => r.typical !== undefined);

export const hostedFacts: [label: string, value: string, href?: string][] = !hostedRun
  ? []
  : [
      ["Workflow run", hostedRun.id, hostedRun.url],
      ["Commit", hostedRun.commit, hostedRun.commitUrl],
      ["Measured", hostedRun.date],
      ["Machine", `GitHub-hosted macos-latest, ${hostedRun.cpu}, ${hostedRun.os}`],
      [
        "Toolchains",
        `Apple clang ${hostedRun.engines.c}, Rust ${hostedRun.engines.rustc}, Go ${hostedRun.engines.go}, OCaml ${hostedRun.engines.ocaml}, Koka ${hostedRun.engines.koka}`,
      ],
      ["Processes", `${hostedRun.processes} fresh processes per cell, the median reported`],
    ];

function nativeTimeTable(): Table {
  if (!hosted)
    return {
      head: [],
      align: [],
      rows: [],
      summary:
        "The first hosted run is still measuring; its times appear here when it finishes. Until then this page shows counts only.",
    };
  return {
    head: ["Program", "n", "Sakura", ...NATIVE_LANGUAGES.map(([, name]) => name)],
    align: ["l", "r", "r", ...NATIVE_LANGUAGES.map(() => "r" as const)],
    code: [0],
    rows: largest(hosted).map(([p, n]) => [
      p,
      grouped(n),
      msWithSpread(get(hostedCells, p, "sakura", "native", n)),
      ...NATIVE_LANGUAGES.map(([lang]) => msWithSpread(get(hostedCells, p, lang, "native", n))),
    ]),
    summary: `Milliseconds per call at the largest size: the median of ${hosted.meta.flags.processes} fresh processes, ± half their range. Typical (geometric mean over the families), Sakura's time over: ${hostedTypical.map((r) => `${r.name} ${factor(r.typical!)}`).join(", ")}. Measured on a GitHub-hosted ${hosted.meta.host.cpu} runner, which other jobs may share, so expect a few percent of noise.`,
  };
}

const bestNative = nativeSummary
  .map((r) => ({ ...r.closest, name: r.name }))
  .toSorted((a, b) => a.ratio - b.ratio)[0];
export const summary: [lead: string, text: string][] = [
  [
    "Native code is young.",
    `For the typical program, native Sakura does ${nativeSummary.map((r) => `${factor(r.typical)} the work of ${r.name}`).join(", ")}. Its best result: ${nativeHeadline(bestNative, bestNative.name)}.`,
  ],
  [
    "Checking and building are fast.",
    [
      check && `\`sakura check\` does ${check.value} ${check.label}`,
      build &&
        `\`sakura build\` builds ${check ? "it" : `a ${grouped(build.lines)}-line project`} in ${ms(build.p95)} ms or less, 19 times out of 20`,
    ]
      .filter(Boolean)
      .join(", and ")
      .concat("."),
  ],
  [
    "Against hand-written JavaScript, running code is mixed.",
    `${nodeWins.length} of ${largest(js).length} programs do less work than the same program written by hand in JavaScript${nodeWins.length ? `, up to ${factor(1 / nodeWins[0].ratio)} less` : ""}. The typical program does ${factor(targetMedian.node)} the work.`,
  ],
  [
    wasmMemory.length ? "WebAssembly saves memory." : "Memory is no lower yet.",
    `${wasmMemory.length} programs built to WebAssembly use at least 10% less peak memory than hand-written JavaScript${wasmMemory.length ? `, up to ${factor(wasmMemory[0].hand / wasmMemory[0].sakura)} less` : ""}.`,
  ],
];

export const tables = {
  winsJs: winsJsTable(),
  winsMemory: winsMemoryTable(),
  typescript: typescriptTable(),
  typescriptStart: typescriptStartTable(),
  js: jsTable(),
  wasm: wasmTable(),
  native: nativeTable(),
  nativeSummary: nativeSummaryTable(),
  nativeTime: nativeTimeTable(),
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
    `Apple clang ${nativePorts.meta.engines.c}`,
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
  [
    "Native baseline",
    `baselines/native/${HOST}.json`,
    `${site.github}/blob/main/bench/baselines/native/${HOST}.json`,
  ],
  [
    "Ports since the bless",
    `results/${NATIVE_PORTS}`,
    `${site.github}/blob/main/bench/results/${NATIVE_PORTS}`,
  ],
  ...(HOSTED
    ? [
        [
          "Hosted wall time",
          `results/${HOSTED}`,
          `${site.github}/blob/main/bench/results/${HOSTED}`,
        ] as [string, string, string],
      ]
    : []),
];

// ---------- Markdown, for the .md and llms routes ----------

const escape = (s: string) => s.replaceAll("|", "\\|");
export function tableMarkdown(t: Table) {
  if (!t.rows.length) return t.summary ? `_${t.summary}_` : "";
  const cell = (s: string, i: number) =>
    t.code?.includes(i) && s !== DASH && !s.includes(" ") ? `\`${s}\`` : escape(s);
  const lines = [
    `| ${t.head.join(" | ")} |`,
    `|${t.align.map((a) => (a === "r" ? "---:" : "---")).join("|")}|`,
    ...t.rows.map((r) => `| ${r.map(cell).join(" | ")} |`),
  ];
  return [lines.join("\n"), t.summary].filter(Boolean).join("\n\n");
}

export const runMarkdown = (facts = runFacts) =>
  facts
    .map(
      ([label, value, href]) =>
        `- **${label}:** ${href ? `[\`${value}\`](${href})` : label === "Node flags" ? `\`${value}\`` : value}`,
    )
    .join("\n");

export const winsMarkdown = () =>
  [...callouts, typical]
    .map(
      (w) =>
        `- **${w.value}** ${w.label}: ${w.subject ? `\`${w.subject}\`, ` : ""}${w.note} (${w.bars.map((b) => `${b.label} ${b.shown}`).join(", ")}).`,
    )
    .join("\n");

export const summaryMarkdown = () =>
  summary.map(([lead, text]) => `- **${lead}** ${text}`).join("\n");

export const chartMarkdown = () =>
  `**Chart:** Sakura's instructions per call over hand JavaScript's, per program at its largest size, on Node and on Bun, with each program's ceiling. The same data is in the target table below.`;
