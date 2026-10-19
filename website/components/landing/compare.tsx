import { RELEASE, releaseLabel } from "@/lib/release";

type Mark = "yes" | "partial" | "no";
type Cell = [Mark, string?];

const LANGS = ["Sakura Lang", "TypeScript", "Python", "Rust", "Effect-TS"] as const;
/** Column heads for the phone matrix, where five full names do not fit. */
const SHORT: Record<(typeof LANGS)[number], string> = {
  "Sakura Lang": "Sakura Lang",
  TypeScript: "TS",
  Python: "Python",
  Rust: "Rust",
  "Effect-TS": "Effect",
};

const ROWS: { label: string; cells: Cell[] }[] = [
  {
    label: "Side effects in the signature",
    cells: [
      ["yes", "checked"],
      ["no", "hidden"],
      ["no", "hidden"],
      ["no", "hidden"],
      ["yes", "R channel"],
    ],
  },
  {
    label: "Errors in the signature",
    cells: [
      ["yes", "Fail<E> error unions"],
      ["no", "untyped throw"],
      ["no", "untyped raise"],
      ["yes", "Result"],
      ["yes", "E channel"],
    ],
  },
  {
    label: "No null, no exceptions",
    cells: [
      ["yes"],
      ["partial", "strict nulls, throw remains"],
      ["no", "None, raise"],
      ["partial", "Option, panics remain"],
      ["partial", "JS throw remains"],
    ],
  },
  {
    label: "Memory without GC pauses or lifetimes",
    cells: [
      ["partial", "host GC today; native RC planned"],
      ["no", "tracing GC"],
      ["partial", "RC plus cycle GC"],
      ["partial", "no GC, lifetimes"],
      ["no", "tracing GC"],
    ],
  },
  {
    label: "Contracts and refinement types",
    cells: [
      ["yes", "checked at runtime, proved by SMT"],
      ["no"],
      ["no", "runtime libraries"],
      ["partial", "external verifiers"],
      ["partial", "Schema, at runtime"],
    ],
  },
  {
    label: "Authority you grant, not ambient",
    cells: [
      ["yes", "handlers grant authority"],
      ["no"],
      ["no"],
      ["no"],
      ["partial", "services, not enforced"],
    ],
  },
  {
    label: "Taint and secret types",
    cells: [
      ["yes", "Untrusted, Secret"],
      ["no"],
      ["no"],
      ["partial", "newtypes by convention"],
      ["partial", "Redacted"],
    ],
  },
  {
    label: "Diagnostics as data, with fixes",
    cells: [
      ["yes", "fixes declare impact"],
      ["partial", "tsserver code fixes"],
      ["partial", "varies by tool"],
      ["yes", "JSON, applicable fixes"],
      ["partial", "via TypeScript"],
    ],
  },
  {
    label: "One way to do each thing",
    cells: [
      ["yes", "small grammar"],
      ["no"],
      ["partial", "by convention"],
      ["no"],
      ["no", "many styles"],
    ],
  },
];

const MARK_LABEL: Record<Mark, string> = { yes: "Yes", partial: "Partly", no: "No" };

function Glyph({ mark }: { mark: Mark }) {
  if (mark === "yes")
    return (
      <svg viewBox="0 0 16 16" className="size-4 shrink-0 text-ok" aria-hidden>
        <circle cx="8" cy="8" r="7" fill="currentColor" opacity="0.16" />
        <path
          d="M4.8 8.3 7 10.4l4.2-4.8"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.7"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    );
  if (mark === "partial")
    return (
      <svg viewBox="0 0 16 16" className="size-4 shrink-0 text-muted-foreground" aria-hidden>
        <circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M8 1.75a6.25 6.25 0 0 1 0 12.5Z" fill="currentColor" />
      </svg>
    );
  return (
    <svg viewBox="0 0 16 16" className="size-4 shrink-0 text-faint" aria-hidden>
      <circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

export function Compare() {
  return (
    <section id="compare" className="border-b border-border bg-muted/50">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <h2 className="max-w-2xl font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
          For code an agent writes, defaults decide.
        </h2>
        <p className="mt-5 max-w-[38rem] text-[17px] leading-relaxed text-pretty text-muted-foreground">
          Every language below can be written carefully. The question is what holds when nobody is
          being careful: what the compiler enforces by default.
        </p>

        <PhoneMatrix />
        <div className="relative mt-12 hidden overflow-x-auto rounded-2xl border border-border bg-card sm:block">
          <table className="w-full min-w-[50rem] sm:min-w-[54rem] border-collapse text-left text-[14px]">
            <caption className="sr-only">
              Comparison of language defaults relevant to agent-written code. The Sakura Lang column
              describes v{RELEASE}.
            </caption>
            <thead>
              <tr className="border-b border-border">
                <th
                  scope="col"
                  className="sticky left-0 z-10 w-[9.5rem] bg-card px-3 py-4 font-medium text-muted-foreground sm:w-[15rem] sm:px-5"
                >
                  By default
                </th>
                {LANGS.map((l, i) => (
                  <th
                    key={l}
                    scope="col"
                    className={`px-4 py-4 font-display text-[15px] font-semibold tracking-[-0.01em] ${i === 0 ? "bg-sakura-wash" : ""}`}
                  >
                    {l}
                    {i === 0 ? (
                      <span className="ml-1.5 font-sans text-[11.5px] font-medium text-sakura-text">
                        {releaseLabel()}
                      </span>
                    ) : null}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {ROWS.map((row) => (
                <tr key={row.label}>
                  <th
                    scope="row"
                    className="sticky left-0 z-10 bg-card px-3 py-3.5 align-top text-[13px] font-medium sm:px-5 sm:text-[14px]"
                  >
                    {row.label}
                  </th>
                  {row.cells.map(([mark, note], i) => (
                    <td
                      key={i}
                      className={`px-4 py-3.5 align-top ${i === 0 ? "bg-sakura-wash" : ""}`}
                    >
                      <span className="flex items-start gap-2">
                        <Glyph mark={mark} />
                        <span className="sr-only">
                          {MARK_LABEL[mark]}
                          {note ? ": " : ""}
                        </span>
                        <span
                          className={`text-[13px] leading-snug ${mark === "no" ? "text-faint" : "text-muted-foreground"} ${i === 0 ? "text-foreground" : ""}`}
                        >
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
        <p className="mt-4 flex flex-wrap gap-x-5 gap-y-1 text-[12.5px] text-faint">
          <span className="inline-flex items-center gap-1.5">
            <Glyph mark="yes" /> enforced by default
          </span>
          <span className="inline-flex items-center gap-1.5">
            <Glyph mark="partial" /> partly, or by convention or library
          </span>
          <span className="inline-flex items-center gap-1.5">
            <Glyph mark="no" /> not provided
          </span>
          <span>The Sakura Lang column describes v{RELEASE}.</span>
        </p>
      </div>
    </section>
  );
}

/**
 * Phones: the same answers as a glyph matrix. The language row sticks under the site header
 * so every column stays labelled while the rows scroll; the notes live in the table above `sm`.
 */
function PhoneMatrix() {
  return (
    <div className="mt-10 sm:hidden">
      <div
        aria-hidden
        className="sticky top-14 z-10 grid grid-cols-5 border-b border-border bg-background/90 py-2.5 text-center font-display text-[13px] font-semibold tracking-[-0.01em] backdrop-blur-md"
      >
        {LANGS.map((l, i) => (
          <span key={l} className={i === 0 ? "text-sakura-text" : ""}>
            {SHORT[l]}
          </span>
        ))}
      </div>
      <ul className="divide-y divide-border">
        {ROWS.map((row) => (
          <li key={row.label} className="pt-3.5 pb-3">
            <p className="text-[14px] leading-snug font-medium">{row.label}</p>
            <div className="mt-2.5 grid grid-cols-5">
              {row.cells.map(([mark, note], i) => (
                <span
                  key={i}
                  className={`grid place-items-center py-1.5 ${i === 0 ? "rounded-full bg-sakura-wash" : ""}`}
                >
                  <Glyph mark={mark} />
                  <span className="sr-only">
                    {LANGS[i]}: {MARK_LABEL[mark]}
                    {note ? `, ${note}` : ""}
                  </span>
                </span>
              ))}
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
