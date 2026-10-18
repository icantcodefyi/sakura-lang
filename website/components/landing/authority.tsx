import Link from "next/link";
import type { ReactNode } from "react";

// Nested scopes, outermost first (design §6.2, §15.6). Each one can only narrow the one around it;
// `reach` is an illustrative share of the host's authority, not a measured quantity.
const SCOPES: { code: ReactNode; note: string; reach: number }[] = [
  { code: "host", note: "everything the machine can do", reach: 100 },
  {
    code: (
      <>
        sakura.toml <span className="text-muted-foreground">[permissions]</span>
      </>
    ),
    note: 'fs.read = ["./data"], net = [], the ceiling',
    reach: 64,
  },
  {
    code: (
      <>
        main() <span className="text-sakura-text">! {"{Console, Cli, Fs, …}"}</span>
      </>
    ),
    note: "root handlers for these effects only",
    reach: 42,
  },
  {
    code: (
      <>
        with Fs.rooted(Dir.subdir(<span className="text-muted-foreground">…, &quot;data&quot;</span>
        ))
      </>
    ),
    note: "paths resolve inside ./data",
    reach: 20,
  },
  {
    code: "word_counts(text) -> Map<String, Int>",
    note: "pure: no row, no authority at all",
    reach: 0,
  },
];

const REFUSED: { code: string; why: string; tag: string }[] = [
  {
    code: 'Fs.read("../.ssh/id_ed25519")',
    why: "outside the rooted directory; symlink escapes are rejected too",
    tag: "FsError",
  },
  {
    code: "main declares Http, net = []",
    why: "refused at startup, not at first use",
    tag: "exit 75",
  },
  {
    code: "a captured Dir outlives its with",
    why: "capabilities cannot escape the scope that made them",
    tag: "V6002",
  },
  {
    code: 'Log.event(Level.Info, "{token}", [])',
    why: "Secret<String> is not Show, so it can never be logged",
    tag: "V2031",
  },
];

/** One row per scope; the bar under each shows how much of the host's authority is left. */
function Scopes(): ReactNode {
  return (
    <ol className="divide-y divide-border rounded-xl border border-strong bg-card px-4 sm:px-5">
      {SCOPES.map((s, i) => (
        <li key={i} className="grid gap-2 py-3.5">
          <p className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5">
            <code className="font-mono text-[11.5px] text-foreground sm:text-[12.5px]">
              {s.code}
            </code>
            <span className="text-[12px] text-muted-foreground sm:text-[12.5px]">{s.note}</span>
          </p>
          <span aria-hidden className="block h-1.5 rounded-full bg-muted">
            <span
              className="block h-full rounded-full bg-sakura"
              style={{ width: `${s.reach}%` }}
            />
          </span>
        </li>
      ))}
    </ol>
  );
}

export function Authority() {
  return (
    <section className="border-b border-border bg-muted/50">
      <div className="mx-auto grid max-w-6xl gap-x-16 gap-y-10 px-5 py-24 sm:px-8 md:py-32 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)] lg:grid-rows-[auto_1fr]">
        <div className="lg:col-start-2 lg:pt-2">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Authority only narrows.
          </h2>
          <p className="mt-5 max-w-[30rem] text-[17px] leading-relaxed text-pretty text-muted-foreground">
            An effect says what a function needs. The handler around it decides how much it gets.
            Root handlers come from the host, capped by{" "}
            <code className="font-mono text-[15px] text-foreground">sakura.toml</code>; every{" "}
            <code className="font-mono text-[15px] text-foreground">with</code> below can narrow
            that, and nothing can widen it. There is no ambient authority to import.
          </p>
        </div>
        <figure className="lg:col-start-1 lg:row-span-2 lg:row-start-1">
          <Scopes />
          <figcaption className="mt-3 text-[12.5px] text-faint">
            Scopes of authority for the CLI example in design §15.6, outermost first. Each one runs
            inside the one above it and can only narrow it.
          </figcaption>
        </figure>
        <div className="lg:col-start-2">
          <ul className="grid max-w-[30rem] divide-y divide-border border-y border-border">
            {REFUSED.map((r) => (
              <li key={r.code} className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 py-3.5">
                <code className="min-w-0 font-mono text-[12.5px] break-words text-foreground">
                  {r.code}
                </code>
                <span className="row-span-2 self-start rounded-full border border-fail/35 bg-fail-wash px-2 py-0.5 font-mono text-[11px] text-fail">
                  {r.tag}
                </span>
                <span className="text-[13.5px] leading-snug text-muted-foreground">{r.why}</span>
              </li>
            ))}
          </ul>
          <Link
            href="/docs/tour/capabilities"
            className="mt-6 inline-block text-[14.5px] font-medium underline decoration-sakura underline-offset-4"
          >
            Capabilities and security
          </Link>
        </div>
      </div>
    </section>
  );
}
