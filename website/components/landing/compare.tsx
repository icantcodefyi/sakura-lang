type Mark = "yes" | "partial" | "no";
type Cell = [Mark, string?];

const LANGS = ["Vow", "TypeScript", "Python", "Rust", "Effect-TS"] as const;

const ROWS: { label: string; cells: Cell[] }[] = [
  {
    label: "Side effects in the signature",
    cells: [["yes", "checked"], ["no", "hidden"], ["no", "hidden"], ["no", "hidden"], ["yes", "R channel"]],
  },
  {
    label: "Errors in the signature",
    cells: [["yes", "Fail<E>"], ["no", "untyped throw"], ["no", "untyped raise"], ["yes", "Result"], ["yes", "E channel"]],
  },
  {
    label: "No null, no exceptions",
    cells: [["yes"], ["partial", "strict nulls, throw remains"], ["no", "None, raise"], ["partial", "Option, panics remain"], ["partial", "JS throw remains"]],
  },
  {
    label: "Memory without GC pauses or lifetimes",
    cells: [["yes", "RC with in-place reuse"], ["no", "tracing GC"], ["partial", "RC plus cycle GC"], ["partial", "no GC, lifetimes"], ["no", "tracing GC"]],
  },
  {
    label: "Contracts and refinement types",
    cells: [["yes", "in the language"], ["no"], ["no", "runtime libraries"], ["partial", "external verifiers"], ["partial", "Schema, at runtime"]],
  },
  {
    label: "Capabilities per dependency",
    cells: [["yes", "granted in vow.toml"], ["no"], ["no"], ["no"], ["partial", "services, not enforced"]],
  },
  {
    label: "Taint and secret types",
    cells: [["yes", "Untrusted, Secret"], ["no"], ["no"], ["partial", "newtypes by convention"], ["partial", "Redacted"]],
  },
  {
    label: "Diagnostics as data, with fixes",
    cells: [["yes", "fix safety levels"], ["partial", "tsserver code fixes"], ["partial", "varies by tool"], ["yes", "JSON, applicable fixes"], ["partial", "via TypeScript"]],
  },
  {
    label: "One way to do each thing",
    cells: [["yes", "small grammar"], ["no"], ["partial", "by convention"], ["no"], ["no", "many styles"]],
  },
];

const MARK_LABEL: Record<Mark, string> = { yes: "Yes", partial: "Partly", no: "No" };

function Glyph({ mark }: { mark: Mark }) {
  if (mark === "yes")
    return (
      <svg viewBox="0 0 16 16" className="size-4 shrink-0 text-ok" aria-hidden>
        <circle cx="8" cy="8" r="7" fill="currentColor" opacity="0.16" />
        <path d="M4.8 8.3 7 10.4l4.2-4.8" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  if (mark === "partial")
    return (
      <svg viewBox="0 0 16 16" className="size-4 shrink-0 text-fg-muted" aria-hidden>
        <circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M8 1.75a6.25 6.25 0 0 1 0 12.5Z" fill="currentColor" />
      </svg>
    );
  return (
    <svg viewBox="0 0 16 16" className="size-4 shrink-0 text-fg-faint" aria-hidden>
      <circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

export function Compare() {
  return (
    <section id="compare" className="border-b border-line bg-bg-sunk/50">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <h2 className="max-w-2xl font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
          For code an agent writes, defaults decide.
        </h2>
        <p className="mt-5 max-w-[38rem] text-[17px] leading-relaxed text-pretty text-fg-muted">
          Every language below can be written carefully. The question is what holds when nobody is being careful: what
          the compiler enforces by default.
        </p>

        <div className="mt-12 overflow-x-auto rounded-2xl border border-line bg-surface">
          <table className="w-full min-w-[54rem] border-collapse text-left text-[14px]">
            <caption className="sr-only">
              Comparison of language defaults relevant to agent-written code. The Vow column describes design goals.
            </caption>
            <thead>
              <tr className="border-b border-line">
                <th scope="col" className="sticky left-0 z-10 w-[15rem] bg-surface px-5 py-4 font-medium text-fg-muted">
                  By default
                </th>
                {LANGS.map((l, i) => (
                  <th
                    key={l}
                    scope="col"
                    className={`px-4 py-4 font-display text-[15px] font-semibold tracking-[-0.01em] ${i === 0 ? "bg-seal-wash" : ""}`}
                  >
                    {l}
                    {i === 0 ? <span className="ml-1.5 font-sans text-[11.5px] font-medium text-seal-text">design</span> : null}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {ROWS.map((row) => (
                <tr key={row.label}>
                  <th scope="row" className="sticky left-0 z-10 bg-surface px-5 py-3.5 font-medium">
                    {row.label}
                  </th>
                  {row.cells.map(([mark, note], i) => (
                    <td key={i} className={`px-4 py-3.5 align-top ${i === 0 ? "bg-seal-wash" : ""}`}>
                      <span className="flex items-start gap-2">
                        <Glyph mark={mark} />
                        <span className="sr-only">{MARK_LABEL[mark]}{note ? ": " : ""}</span>
                        <span className={`text-[13px] leading-snug ${mark === "no" ? "text-fg-faint" : "text-fg-muted"} ${i === 0 ? "text-fg" : ""}`}>
                          {note ?? (mark === "no" ? "" : MARK_LABEL[mark])}
                        </span>
                      </span>
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="mt-4 flex flex-wrap gap-x-5 gap-y-1 text-[12.5px] text-fg-faint">
          <span className="inline-flex items-center gap-1.5"><Glyph mark="yes" /> enforced by default</span>
          <span className="inline-flex items-center gap-1.5"><Glyph mark="partial" /> partly, or by convention or library</span>
          <span className="inline-flex items-center gap-1.5"><Glyph mark="no" /> not provided</span>
          <span>The Vow column states design goals; nothing is shipped yet.</span>
        </p>
      </div>
    </section>
  );
}
