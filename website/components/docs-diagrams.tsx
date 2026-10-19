// Flat diagrams for the docs: boxes on a grid joined by orthogonal arrows, drawn as inline SVG on
// the server. Each diagram has a wide layout and a narrow one (for phones); a container query
// picks one, and the SVG's viewBox reserves its height, so nothing shifts and no script runs.
// Colours are the theme's CSS variables. lib/source.ts prints the same data as Markdown for the
// .md and llms routes, so the picture and the text never drift.

// `effect` marks authority, `ok` a good end, `fail` a failure or defect, `exp` an experimental
// path (dashed). Every outcome is also said in words in its note.
type Tone = "effect" | "ok" | "fail" | "exp";
// [column, row, span]; a half column centres a box between two, and a span (default 1) widens a
// box over that many columns.
type At = [col: number, row: number, span?: number];
type Node = {
  id: string;
  /** Wrapped by `wrap`; a no-break space (\u00a0) keeps a group such as `{ id }` whole. */
  code: string;
  note: string;
  tone?: Tone;
  wide: At;
  narrow: At;
  /** A short label above the box, starting a group of rows. */
  section?: string;
};
// Edges run along the layout's flow; an edge pointing back along it loops around the outside.
type Edge = [from: string, to: string];
type Flow = "right" | "down";
type Diagram = {
  caption: string;
  nodes: Node[];
  edges: Edge[];
  after?: string;
  flow?: { wide: Flow; narrow: Flow };
};

const DIAGRAMS = {
  PipelineFlow: {
    caption:
      "The compiler pipeline on main (design §13). One Core feeds every backend; WASM and native share the LIR.",
    flow: { wide: "down", narrow: "down" },
    nodes: [
      {
        id: "src",
        code: "source",
        note: ".sak files, parsed with error recovery",
        wide: [0, 0],
        narrow: [0.5, 0],
      },
      {
        id: "cst",
        code: "CST",
        note: "lossless; the formatter works here",
        wide: [1, 0],
        narrow: [0.5, 1],
      },
      { id: "hir", code: "HIR", note: "names resolved", wide: [2, 0], narrow: [0.5, 2] },
      {
        id: "typed",
        code: "Typed HIR",
        note: "types, effect rows, holes, obligations",
        wide: [3, 0],
        narrow: [0.5, 3],
      },
      {
        id: "core",
        code: "Core",
        note: "explicit Perform, Handle, Fail, Check",
        wide: [4, 0],
        narrow: [0.5, 4],
      },
      {
        id: "js",
        code: "JavaScript",
        note: "the default target; runs on Node",
        tone: "ok",
        wide: [4, 1],
        narrow: [0, 5],
      },
      {
        id: "lir",
        code: "LIR",
        note: "from a Core snapshot; reference counts made explicit",
        wide: [3, 1],
        narrow: [1, 5],
      },
      {
        id: "wasm",
        code: "WebAssembly",
        note: "complete; runs on Node",
        tone: "ok",
        wide: [3, 2],
        narrow: [1, 6],
      },
      {
        id: "native",
        code: "native",
        note: "Cranelift; experimental, behind a feature",
        tone: "exp",
        wide: [2, 2],
        narrow: [0, 6],
      },
    ],
    edges: [
      ["src", "cst"],
      ["cst", "hir"],
      ["hir", "typed"],
      ["typed", "core"],
      ["core", "js"],
      ["core", "lir"],
      ["lir", "wasm"],
      ["lir", "native"],
    ],
    after:
      "Select a backend with --target js (the default) or --target wasm. Native code is experimental: only a compiler built with the native Cargo feature accepts --target native, and no release includes it.",
  },
  EffectFlow: {
    caption:
      "Rows add up through calls: a function's row is the union of what its body performs, and a handler subtracts what it handles.",
    nodes: [
      { id: "clock", code: "Clock.now()", note: "performs Clock", wide: [0, 0], narrow: [0, 0] },
      {
        id: "print",
        code: "Console.print(…)",
        note: "performs Console",
        wide: [0, 1],
        narrow: [1, 0],
      },
      {
        id: "greet",
        code: "fn greet(…) ! {Console, Clock}",
        note: "the union of its body; a missing label is V3001",
        tone: "effect",
        wide: [1, 0.5],
        narrow: [0.5, 1],
      },
      {
        id: "main",
        code: "fn main() ! {Console, Clock}",
        note: "calling greet performs greet's row",
        wide: [2, 0],
        narrow: [0, 2],
      },
      {
        id: "with",
        code: "with Clock.fixed(t) { greet(…) }",
        note: "the handler takes Clock; {Console} is left",
        tone: "ok",
        wide: [2, 1],
        narrow: [1, 2],
      },
    ],
    edges: [
      ["clock", "greet"],
      ["print", "greet"],
      ["greet", "main"],
      ["greet", "with"],
    ],
    after:
      "Order does not matter: a row is a set. main gets Console and Clock from the host's root handlers; a with handler removes the effects it handles from the row around it.",
  },
  FailureFlow: {
    caption:
      "A typed failure is part of the signature and can be recovered; a defect is a bug and ends the task.",
    flow: { wide: "right", narrow: "down" },
    nodes: [
      {
        id: "fail",
        code: "fail NotFound { id }",
        note: "adds Fail<NotFound> to the row",
        section: "typed failure",
        wide: [0, 0],
        narrow: [0, 0],
      },
      {
        id: "catch",
        code: "catch { NotFound {\u00a0id\u00a0} => … }",
        note: "a matching arm recovers; the tag leaves the row",
        tone: "ok",
        wide: [1, 0],
        narrow: [1, 0],
      },
      {
        id: "up",
        code: "caller: Fail<NotFound>",
        note: "no arm matches: it propagates; out of main, exit 1",
        wide: [1, 1],
        narrow: [1, 1],
      },
      {
        id: "defect",
        code: "D002",
        note: "a bug such as division by zero; try cannot catch it",
        tone: "fail",
        section: "defect",
        wide: [0, 2],
        narrow: [0, 2],
      },
      {
        id: "final",
        code: "finalizers",
        note: "open resources are closed",
        wide: [1, 2],
        narrow: [1, 2],
      },
      {
        id: "end",
        code: "Outcome.Defect(info)",
        note: "the task ends; out of main, exit 70",
        tone: "fail",
        wide: [2, 2],
        narrow: [1, 3],
      },
    ],
    edges: [
      ["fail", "catch"],
      ["fail", "up"],
      ["defect", "final"],
      ["final", "end"],
    ],
    after:
      "try with no catch turns the failure into a Result<T, E>, and ? turns a Result back into Fail<E>. Task.scope and the test runner observe a defect as Outcome.Defect.",
  },
  HandlerFlow: {
    caption:
      "The innermost handler's clause runs in place of the operation and returns its result once.",
    nodes: [
      {
        id: "op",
        code: "Counter.next()",
        note: "the body performs an operation",
        wide: [0, 0],
        narrow: [0, 0, 2],
      },
      {
        id: "with",
        code: "with memory_counter(0)",
        note: "the innermost Counter handler takes it",
        tone: "effect",
        wide: [1, 0],
        narrow: [0, 1, 2],
      },
      {
        id: "clause",
        code: "fn next()",
        note: "its clause runs in place of the call",
        wide: [2, 0],
        narrow: [0, 2, 2],
      },
      {
        id: "result",
        code: "1",
        note: "the clause's value is the call's result",
        tone: "ok",
        wide: [3, 0],
        narrow: [0, 3],
      },
      {
        id: "outer",
        code: "outer handlers",
        note: "effects the clause performs go outside its with",
        wide: [2, 1],
        narrow: [1, 3],
      },
    ],
    edges: [
      ["op", "with"],
      ["with", "clause"],
      ["clause", "result"],
      ["clause", "outer"],
    ],
    after:
      "There is no resume: the clause's value goes back to the call exactly once. A handler for the same effect further out is shadowed.",
  },
  AuthorityFlow: {
    caption:
      "The CLI example in design §15.6. Effects are what code needs; each handler can only narrow the authority of the one outside it.",
    nodes: [
      {
        id: "manifest",
        code: "sakura.toml [permissions]",
        note: "the ceiling: read ./data, no network",
        wide: [0, 0],
        narrow: [0, 0],
      },
      {
        id: "root",
        code: "main() ! {Console, Cli, Fs, …}",
        note: "root handlers for these std effects only",
        tone: "effect",
        wide: [1, 0],
        narrow: [0, 1],
      },
      {
        id: "rooted",
        code: 'with Fs.rooted(Dir.subdir(…, "data"))',
        note: "paths resolve inside ./data",
        tone: "effect",
        wide: [2, 0],
        narrow: [0, 2],
      },
      {
        id: "op",
        code: "Fs.read_text(path)",
        note: "a function with Fs in its row reaches ./data only",
        tone: "ok",
        wide: [3, 0],
        narrow: [0, 3],
      },
      {
        id: "75",
        code: "exit 75",
        note: "main declares Http with net = []: refused at startup",
        tone: "fail",
        wide: [1, 1],
        narrow: [1, 1],
      },
      {
        id: "V6002",
        code: "V6002",
        note: "a captured Dir cannot outlive its with",
        tone: "fail",
        wide: [2, 1],
        narrow: [1, 2],
      },
    ],
    edges: [
      ["manifest", "root"],
      ["root", "rooted"],
      ["rooted", "op"],
      ["root", "75"],
      ["rooted", "V6002"],
    ],
    after:
      "Fs in a signature says the function may use the filesystem, not which files. A path outside ./data fails with FsError, symlink escapes included. A pure function such as word_counts(text) has no row and reaches nothing.",
  },
  ContractFlow: {
    caption: "Where each clause of a call to clamp is checked, and who is blamed when one fails.",
    nodes: [
      {
        id: "call",
        code: "clamp(x, lo, hi)",
        note: "the caller passes arguments",
        wide: [0, 0],
        narrow: [0, 0],
      },
      {
        id: "req",
        code: "requires ordered",
        note: "checked on entry",
        wide: [1, 0],
        narrow: [0, 1],
      },
      {
        id: "ens",
        code: "ensures bounded",
        note: "checked when the body returns",
        wide: [2, 0],
        narrow: [0, 2],
      },
      {
        id: "ret",
        code: "result",
        note: "returned to the caller",
        tone: "ok",
        wide: [3, 0],
        narrow: [0, 3],
      },
      {
        id: "D020",
        code: "D020",
        note: "violated: the caller is blamed",
        tone: "fail",
        wide: [1, 1],
        narrow: [1, 1],
      },
      {
        id: "D021",
        code: "D021",
        note: "violated: clamp's body is blamed",
        tone: "fail",
        wide: [2, 1],
        narrow: [1, 2],
      },
    ],
    edges: [
      ["call", "req"],
      ["req", "ens"],
      ["ens", "ret"],
      ["req", "D020"],
      ["ens", "D021"],
    ],
    after:
      "A where refinement is checked when a value enters the type. A scalar literal is decided at compile time when the bounded evaluator can (V5004 on rejection); every other check stays at runtime unless sakura verify proves it.",
  },
  VerifyFlow: {
    caption:
      "sakura verify gives each obligation one of four results; only a proof can remove a check.",
    flow: { wide: "right", narrow: "down" },
    nodes: [
      {
        id: "verify",
        code: "sakura verify",
        note: "contract and arithmetic obligations go to pinned Z3 workers",
        wide: [0, 1.5],
        narrow: [0.5, 0],
      },
      {
        id: "proved",
        code: "proved",
        note: "holds under the recorded assumptions; a build may drop an eligible check",
        tone: "ok",
        wide: [1, 0],
        narrow: [0, 1],
      },
      {
        id: "disproved",
        code: "disproved",
        note: "a counterexample exists; the run exits 1",
        tone: "fail",
        wide: [1, 1],
        narrow: [1, 1],
      },
      {
        id: "unknown",
        code: "unknown",
        note: "the solver could not decide; the check stays",
        wide: [1, 2],
        narrow: [0, 2],
      },
      {
        id: "unsupported",
        code: "unsupported",
        note: "outside the implemented fragment; the check stays",
        wide: [1, 3],
        narrow: [1, 2],
      },
    ],
    edges: [
      ["verify", "proved"],
      ["verify", "disproved"],
      ["verify", "unknown"],
      ["verify", "unsupported"],
    ],
    after:
      "The exit code is 0 for unknown and unsupported too: automation that needs every proof reads the JSON Lines records.",
  },
  AgentLoop: {
    caption:
      "The agent loop: check, read structured records, fix by impact, and check again until clean.",
    nodes: [
      {
        id: "write",
        code: "{ ?body }",
        note: "signatures first; bodies as holes",
        wide: [0, 0],
        narrow: [0, 0],
      },
      {
        id: "check",
        code: "sakura check --json",
        note: "one JSON Lines record per hole or problem",
        tone: "effect",
        wide: [1, 0],
        narrow: [0, 1],
      },
      {
        id: "fix",
        code: "sakura fix",
        note: "fill holes; apply fixes by impact class",
        wide: [2, 1],
        narrow: [1, 2],
      },
      {
        id: "test",
        code: "sakura test --json",
        note: "once check is clean",
        wide: [2, 0],
        narrow: [0, 2],
      },
      {
        id: "build",
        code: "sakura build",
        note: "rejects any hole still left",
        tone: "ok",
        wide: [3, 0],
        narrow: [0, 3],
      },
    ],
    edges: [
      ["write", "check"],
      ["check", "test"],
      ["check", "fix"],
      ["fix", "check"],
      ["test", "build"],
    ],
    after: "Report every signature, effect, error or permission change to the reviewer.",
  },
  PatchFlow: {
    caption:
      "Patching through MCP: every result names the snapshot it was computed from, so a stale patch conflicts instead of landing on the wrong text.",
    nodes: [
      {
        id: "check",
        code: "check(snapshot)",
        note: "diagnostics and holes; hole_context for one hole",
        tone: "effect",
        wide: [0, 0],
        narrow: [0, 0],
      },
      {
        id: "try",
        code: "check_patch(edits, frozen_specs)",
        note: "diagnostics and the interface delta; nothing changes",
        wide: [1, 0],
        narrow: [0, 1],
      },
      {
        id: "apply",
        code: "apply_patch(patch_id, frozen_specs)",
        note: "commits it and returns a new snapshot",
        tone: "ok",
        wide: [2, 0],
        narrow: [0, 2],
      },
      {
        id: "conflict",
        code: "conflict",
        note: "the workspace moved on; nothing is applied",
        tone: "fail",
        wide: [2, 1],
        narrow: [1, 2],
      },
      {
        id: "test",
        code: "test(snapshot)",
        note: "tests under deterministic handlers",
        wide: [3, 0],
        narrow: [0, 3],
      },
    ],
    edges: [
      ["check", "try"],
      ["try", "apply"],
      ["apply", "test"],
      ["apply", "conflict"],
      ["test", "check"],
      ["conflict", "check"],
    ],
    after:
      'Frozen specs are opt-in per call: pass frozen_specs: "all" to both check_patch and apply_patch, and a change to a frozen signature, row, contract or permission is V8020.',
  },
  CassetteFlow: {
    caption: "Recording is opt-in; ordinary tests replay the cassette with no network.",
    flow: { wide: "right", narrow: "down" },
    nodes: [
      {
        id: "r1",
        code: "Http.send(req)",
        note: "the test sends a request",
        section: "sakura test --record",
        wide: [0, 0],
        narrow: [0, 0],
      },
      {
        id: "r2",
        code: "Http.recording",
        note: "no matching exchange",
        wide: [1, 0],
        narrow: [0, 1],
      },
      {
        id: "r3",
        code: "outer Http",
        note: "forwards to a manifest net host",
        tone: "effect",
        wide: [2, 0],
        narrow: [0, 2],
      },
      {
        id: "r4",
        code: "cassette.jsonl",
        note: "the exchange is appended under tests/fixtures/",
        wide: [3, 0],
        narrow: [0, 3],
      },
      {
        id: "p1",
        code: "Http.send(req)",
        note: "the same request",
        section: "sakura test",
        wide: [0, 1],
        narrow: [0, 4],
      },
      {
        id: "p2",
        code: "Http.recording",
        note: "matches method, URL, headers, body",
        wide: [1, 1],
        narrow: [0, 5],
      },
      {
        id: "p3",
        code: "cassette.jsonl",
        note: "the recorded response comes back",
        tone: "ok",
        wide: [2, 1],
        narrow: [0, 6],
      },
    ],
    edges: [
      ["r1", "r2"],
      ["r2", "r3"],
      ["r3", "r4"],
      ["p1", "p2"],
      ["p2", "p3"],
    ],
    after: "Without --record, a miss fails with HttpError pointing to sakura test --record.",
  },
} satisfies Record<string, Diagram>;

type Name = keyof typeof DIAGRAMS;

// Geometry, in SVG units (1 unit = 1 CSS px at the layout's natural width).
const LAYOUT = {
  wide: { width: 720, colGap: 36, rowGap: 30 },
  narrow: { width: 320, colGap: 18, rowGap: 26 },
} as const;
type LayoutName = keyof typeof LAYOUT;
const PAD = { x: 11, y: 10 };
const CODE = { size: 14, line: 19, em: 0.632 }; // Atkinson Hyperlegible Mono advance, measured
const NOTE = { size: 13.5, line: 18, em: 0.52 }; // a slightly generous average for the body face
const SECTION = 24; // room above a row that starts a section
const LANE = 26; // room outside the grid for edges that loop back

/**
 * Greedy word wrap by an estimated character budget. A word too long for a line breaks after a
 * `(`, `.` or `,` inside it, as code would, or before the `(` when `name(` alone is too long, so
 * an identifier is never split; otherwise words stay whole.
 */
function wrap(text: string, chars: number) {
  const lines: string[] = [];
  for (const word of text.split(" ")) {
    let parts = word.length > chars ? word.split(/(?<=[(.,])/) : [word];
    if (parts.some((p) => p.length > chars)) parts = word.split(/(?<=[.,])|(?=\()/);
    parts.forEach((part, i) => {
      const last = lines.at(-1);
      const glue = i === 0 ? " " : "";
      if (last !== undefined && last.length + glue.length + part.length <= chars)
        lines[lines.length - 1] = last + glue + part;
      else lines.push(part);
    });
  }
  return lines;
}

type Box = Node & {
  x: number;
  y: number;
  w: number;
  h: number;
  codeLines: string[];
  noteLines: string[];
};

function layout(d: Diagram, which: LayoutName) {
  const { width, colGap, rowGap } = LAYOUT[which];
  const flow = d.flow?.[which] ?? (which === "wide" ? "right" : "down");
  const at = (n: Node) => n[which];
  const back = d.edges.some(([a, b]) => isBack(at(find(d, a)), at(find(d, b)), flow));
  const span = (n: Node) => at(n)[2] ?? 1;
  const cols = Math.max(...d.nodes.map((n) => at(n)[0] + span(n)));
  const w = (width - (back && flow === "down" ? LANE : 0) - (cols - 1) * colGap) / cols;
  const boxW = (n: Node) => span(n) * w + (span(n) - 1) * colGap;
  const wrapped = d.nodes.map((n) => ({
    n,
    codeLines: wrap(n.code, Math.floor((boxW(n) - 2 * PAD.x) / (CODE.size * CODE.em))),
    noteLines: wrap(n.note, Math.floor((boxW(n) - 2 * PAD.x) / (NOTE.size * NOTE.em))),
  }));
  const need = (b: (typeof wrapped)[number]) =>
    2 * PAD.y + b.codeLines.length * CODE.line + 4 + b.noteLines.length * NOTE.line;
  // Boxes in a row share its height; a box on a half row sits centred between two rows.
  const rows = Math.floor(Math.max(...d.nodes.map((n) => at(n)[1]))) + 1;
  const rowH = Array.from({ length: rows }, (_, r) =>
    Math.max(0, ...wrapped.filter((b) => at(b.n)[1] === r).map(need)),
  );
  const tops: number[] = [];
  for (let r = 0, y = 0; r < rows; r++) {
    if (d.nodes.some((n) => n.section && at(n)[1] === r)) y += SECTION;
    tops.push(y);
    y += rowH[r] + rowGap;
  }
  const boxes: Box[] = wrapped.map((b) => {
    const [col, row] = at(b.n);
    const whole = Number.isInteger(row);
    const h = whole ? rowH[row] : need(b);
    const lo = Math.floor(row);
    const rowSpan = whole ? 0 : tops[lo + 1] + rowH[lo + 1] - tops[lo];
    return {
      ...b.n,
      x: col * (w + colGap),
      y: whole ? tops[row] : tops[lo] + (rowSpan - h) / 2,
      w: boxW(b.n),
      h,
      codeLines: b.codeLines,
      noteLines: b.noteLines,
    };
  });
  const right = Math.max(...boxes.map((b) => b.x + b.w));
  const bottom = Math.max(...boxes.map((b) => b.y + b.h));
  return {
    flow,
    boxes,
    width,
    height: bottom + (back && flow === "right" ? LANE : 0) + 2,
    right,
    bottom,
    rowGap,
    colGap,
  };
}

const find = (d: Diagram, id: string) => {
  const n = d.nodes.find((x) => x.id === id);
  if (!n) throw new Error(`diagram edge names an unknown node: ${id}`);
  return n;
};

const isBack = (a: At, b: At, flow: Flow) => (flow === "right" ? b[0] < a[0] : b[1] < a[1]);

type Pt = [number, number];

/** An orthogonal route from a to b: along the flow, an elbow between rows/columns, or a loop. */
function route(a: Box, b: Box, l: ReturnType<typeof layout>): Pt[] {
  const cx = (n: Box) => n.x + n.w / 2;
  const cy = (n: Box) => n.y + n.h / 2;
  const sameCol = Math.abs(a.x - b.x) < 1;
  const sameRow = Math.abs(a.y - b.y) < 1;
  if (l.flow === "right") {
    if (sameCol)
      return b.y > a.y
        ? [
            [cx(a), a.y + a.h],
            [cx(b), b.y],
          ]
        : [
            [cx(a), a.y],
            [cx(b), b.y + b.h],
          ];
    if (b.x > a.x) {
      const mid = b.x - l.colGap / 2;
      return sameRow
        ? [
            [a.x + a.w, cy(a)],
            [b.x, cy(b)],
          ]
        : [
            [a.x + a.w, cy(a)],
            [mid, cy(a)],
            [mid, cy(b)],
            [b.x, cy(b)],
          ];
    }
    const lane = l.bottom + LANE / 2;
    return [
      [cx(a), a.y + a.h],
      [cx(a), lane],
      [cx(b), lane],
      [cx(b), b.y + b.h],
    ];
  }
  if (sameRow)
    return b.x > a.x
      ? [
          [a.x + a.w, cy(a)],
          [b.x, cy(b)],
        ]
      : [
          [a.x, cy(a)],
          [b.x + b.w, cy(b)],
        ];
  if (b.y > a.y) {
    if (Math.abs(cx(a) - cx(b)) < 1)
      return [
        [cx(a), a.y + a.h],
        [cx(b), b.y],
      ];
    const mid = b.y - l.rowGap / 2;
    return [
      [cx(a), a.y + a.h],
      [cx(a), mid],
      [cx(b), mid],
      [cx(b), b.y],
    ];
  }
  const lane = l.right + LANE / 2;
  return [
    [a.x + a.w, cy(a)],
    [lane, cy(a)],
    [lane, cy(b)],
    [b.x + b.w, cy(b)],
  ];
}

const ARROW = 6;

function EdgePath({ pts }: { pts: Pt[] }) {
  // Stop the line short of the box so the arrowhead's tip touches it.
  const [px, py] = pts[pts.length - 2];
  const [ex, ey] = pts[pts.length - 1];
  const dx = Math.sign(ex - px);
  const dy = Math.sign(ey - py);
  const line = [...pts.slice(0, -1), [ex - dx * ARROW, ey - dy * ARROW]];
  const head = [
    [ex, ey],
    [ex - dx * ARROW - dy * (ARROW / 1.6), ey - dy * ARROW - dx * (ARROW / 1.6)],
    [ex - dx * ARROW + dy * (ARROW / 1.6), ey - dy * ARROW + dx * (ARROW / 1.6)],
  ];
  return (
    <g className="text-muted-foreground">
      <polyline
        points={line.join(" ")}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.25}
        strokeLinejoin="round"
      />
      <polygon points={head.join(" ")} fill="currentColor" />
    </g>
  );
}

const TONE: Record<Tone | "plain", { box: string; code: string; dash?: string }> = {
  plain: { box: "fill-card stroke-strong", code: "fill-foreground" },
  effect: { box: "fill-sakura-wash stroke-sakura", code: "fill-sakura-text" },
  ok: { box: "fill-ok-wash stroke-ok/50", code: "fill-ok" },
  fail: { box: "fill-fail-wash stroke-fail/50", code: "fill-fail" },
  exp: { box: "fill-background stroke-strong", code: "fill-foreground", dash: "4 3" },
};

function NodeBox({ b }: { b: Box }) {
  const t = TONE[b.tone ?? "plain"];
  const noteTop = b.y + PAD.y + b.codeLines.length * CODE.line + 4;
  return (
    <g>
      {b.section && (
        <text x={b.x} y={b.y - 9} className="fill-sakura-text font-mono text-[13px]">
          {b.section}
        </text>
      )}
      <rect
        x={b.x + 0.5}
        y={b.y + 0.5}
        width={b.w - 1}
        height={b.h - 1}
        rx={6}
        strokeWidth={1}
        strokeDasharray={t.dash}
        className={t.box}
      />
      <text className={`font-mono ${t.code}`} fontSize={CODE.size}>
        {b.codeLines.map((line, i) => (
          <tspan key={i} x={b.x + PAD.x} y={b.y + PAD.y + CODE.size + i * CODE.line - 1}>
            {line}
          </tspan>
        ))}
      </text>
      <text className="fill-muted-foreground font-sans" fontSize={NOTE.size}>
        {b.noteLines.map((line, i) => (
          <tspan key={i} x={b.x + PAD.x} y={noteTop + NOTE.size + i * NOTE.line - 1}>
            {line}
          </tspan>
        ))}
      </text>
    </g>
  );
}

function Svg({ name, which, className }: { name: Name; which: LayoutName; className: string }) {
  const d: Diagram = DIAGRAMS[name];
  const l = layout(d, which);
  const byId = new Map(l.boxes.map((b) => [b.id, b]));
  const id = `diagram-${name}-${which}`;
  return (
    <svg
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- inline SVG keeps the theme colours and real text; an <img> can't
      role="img"
      aria-labelledby={`${id}-title ${id}-desc`}
      viewBox={`0 0 ${l.width} ${Math.ceil(l.height)}`}
      width={l.width}
      height={Math.ceil(l.height)}
      className={`h-auto w-full overflow-visible ${className}`}
    >
      <title id={`${id}-title`}>{d.caption}</title>
      <desc id={`${id}-desc`}>
        {steps(d)
          .map((s) => (s.section ? `${s.section}: ${s.text}` : s.text))
          .join(" ")}
      </desc>
      {d.edges.map(([a, b]) => (
        <EdgePath key={`${a}-${b}`} pts={route(byId.get(a)!, byId.get(b)!, l)} />
      ))}
      {l.boxes.map((b) => (
        <NodeBox key={b.id} b={b} />
      ))}
    </svg>
  );
}

function Figure({ name }: { name: Name }) {
  const d: Diagram = DIAGRAMS[name];
  // The wide layout needs about 620px; narrower containers (phones, the tablet docs column) get
  // the narrow one. Each SVG caps at its natural width so text never grows past its size.
  return (
    <figure data-diagram className="not-prose @container my-8 grid gap-4">
      <Svg name={name} which="wide" className="hidden max-w-[720px] @min-[620px]:block" />
      <Svg name={name} which="narrow" className="max-w-[400px] @min-[620px]:hidden" />
      {d.after && <p className="text-[13.5px] leading-relaxed text-muted-foreground">{d.after}</p>}
      <figcaption className="border-t border-border pt-2.5 text-[13px] text-faint">
        {d.caption}
      </figcaption>
    </figure>
  );
}

export const PipelineFlow = () => <Figure name="PipelineFlow" />;
export const EffectFlow = () => <Figure name="EffectFlow" />;
export const FailureFlow = () => <Figure name="FailureFlow" />;
export const HandlerFlow = () => <Figure name="HandlerFlow" />;
export const AuthorityFlow = () => <Figure name="AuthorityFlow" />;
export const ContractFlow = () => <Figure name="ContractFlow" />;
export const VerifyFlow = () => <Figure name="VerifyFlow" />;
export const AgentLoop = () => <Figure name="AgentLoop" />;
export const PatchFlow = () => <Figure name="PatchFlow" />;
export const CassetteFlow = () => <Figure name="CassetteFlow" />;

/**
 * The diagram in words: each box in order, then where its arrows go when that is not simply the
 * next box, or that its path ends. `format` formats a code label (backticks for Markdown, nothing
 * for the SVG's desc).
 */
function steps(d: Diagram, format = (s: string) => s) {
  // No-break spaces only steer the SVG's wrapping; the text gets plain spaces.
  const code = (s: string) => format(s.replaceAll("\u00a0", " "));
  return d.nodes.map((n, i) => {
    const to = d.edges.filter(([a]) => a === n.id).map(([, b]) => b);
    const then = !to.length
      ? i < d.nodes.length - 1
        ? " This path ends here."
        : ""
      : to.length === 1 && to[0] === d.nodes[i + 1]?.id
        ? ""
        : ` Then ${to.map((b) => code(find(d, b).code)).join(" or ")}.`;
    return { section: n.section, text: `${code(n.code)}: ${n.note}.${then}` };
  });
}

/** The diagram as a Markdown quote, for the .md and llms routes. */
function markdown(d: Diagram) {
  const items = steps(d, (s) => `\`${s}\``)
    .map((s, i) => `${s.section ? `${i ? "\n" : ""}\`${s.section}\`:\n\n` : ""}${i + 1}. ${s.text}`)
    .join("\n");
  return [`**Diagram:** ${d.caption}`, items, d.after]
    .filter(Boolean)
    .join("\n\n")
    .split("\n")
    .map((l) => (l ? `> ${l}` : ">"))
    .join("\n");
}

export const diagramMarkdown = Object.fromEntries(
  Object.entries(DIAGRAMS).map(([name, d]) => [name, () => markdown(d)]),
) as Record<Name, () => string>;
