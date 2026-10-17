import Link from "next/link";

// Milestones from docs/design.md §14.3. "works": its acceptance programs run on JavaScript today.
const PHASES: { id: string; name: string; body: string; status?: "works" | "now" }[] = [
  {
    id: "M0",
    name: "Hello, effects",
    body: "Lexer, parser, Console, inference, CPS JavaScript, sakura run and sakura check --json.",
    status: "works",
  },
  {
    id: "M1",
    name: "Pure language, typed errors",
    body: "Records, variants, exhaustive match, Fail with try/catch, std in Sakura Lang, tests, sakura fmt.",
    status: "works",
  },
  {
    id: "M2",
    name: "Handlers and holes",
    body: "User effects and handlers, row variables, deterministic test handlers, typed holes.",
    status: "works",
  },
  {
    id: "M3",
    name: "Contracts and agents",
    body: "Runtime contracts, permissions, Secret and Untrusted, Task, Http, Sql, LSP and MCP.",
    status: "now",
  },
  {
    id: "M4",
    name: "WASM and verify",
    body: "WASM backend; sakura verify discharges contracts with Z3 and drops proved checks.",
  },
  {
    id: "M5",
    name: "Native",
    body: "Cranelift backend with Perceus reference counting and in-place reuse.",
  },
];

export function Roadmap() {
  return (
    <section id="roadmap" className="border-b border-line">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="flex flex-col justify-between gap-6 md:flex-row md:items-end">
          <h2 className="max-w-xl font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Designed. Running in part.
          </h2>
          <p className="max-w-sm text-[15px] leading-relaxed text-fg-muted">
            The compiler checks and runs M0 to M2 programs on JavaScript today. M3 is under way, in
            the open, and nothing is released yet.{" "}
            <Link
              href="/docs/roadmap"
              className="font-medium text-fg underline decoration-seal underline-offset-4"
            >
              Full roadmap
            </Link>
          </p>
        </div>

        <ol className="relative mt-14 grid gap-8 md:grid-cols-3 lg:grid-cols-6 lg:gap-5">
          <span
            aria-hidden
            className="absolute top-[7px] right-0 left-0 hidden h-px bg-line-strong lg:block"
          />
          <span
            aria-hidden
            className="absolute top-0 bottom-0 left-[7px] w-px bg-line-strong md:hidden"
          />
          {PHASES.map((p) => (
            <li key={p.id} className="relative pl-9 md:pl-0">
              <span
                aria-hidden
                className={`absolute top-0 left-0 grid size-[15px] place-items-center rounded-full border ${
                  p.status === "now"
                    ? "border-seal bg-seal"
                    : p.status === "works"
                      ? "border-ok bg-ok text-bg"
                      : "border-line-strong bg-bg"
                } md:relative`}
              >
                {p.status === "now" ? (
                  <span className="absolute inset-[-5px] rounded-full border border-seal/50 motion-safe:animate-[ping_2.4s_var(--ease-out)_infinite]" />
                ) : p.status === "works" ? (
                  <svg viewBox="0 0 12 12" className="size-[9px]">
                    <path
                      d="M2.5 6.3 5 8.6l4.5-5"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                ) : null}
              </span>
              <p className="font-mono text-[11.5px] text-fg-faint md:mt-5">
                {p.id}
                {p.status === "now" ? (
                  <span className="ml-2 rounded-full bg-seal-wash px-2 py-0.5 font-sans font-medium text-seal-text">
                    In progress
                  </span>
                ) : p.status === "works" ? (
                  <span className="ml-2 rounded-full bg-ok-wash px-2 py-0.5 font-sans font-medium text-ok">
                    Runs today
                  </span>
                ) : null}
              </p>
              <h3 className="mt-2 font-display text-[1.15rem] font-semibold tracking-[-0.015em]">
                {p.name}
              </h3>
              <p className="mt-1.5 text-[14px] leading-relaxed text-fg-muted">{p.body}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}
