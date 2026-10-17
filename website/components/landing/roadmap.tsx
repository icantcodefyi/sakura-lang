import Link from "next/link";

const PHASES = [
  { name: "Design", body: "Language design document, prior-art research, this site.", now: true },
  {
    name: "Checker",
    body: "Parser, type and effect checker, JSON diagnostics, typed holes, interpreter.",
  },
  {
    name: "Verification",
    body: "Contracts, refinement solving, property tests, deterministic simulation.",
  },
  {
    name: "Code generation",
    body: "Reference-counted IR with reuse; native, WASM and JS backends.",
  },
  {
    name: "Batteries",
    body: "Standard library, HTTP, typed SQL, web and RPC, workflows, Llm, MCP.",
  },
  { name: "Toward 1.0", body: "Stability policy and a package registry." },
];

export function Roadmap() {
  return (
    <section id="roadmap" className="border-b border-line">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="flex flex-col justify-between gap-6 md:flex-row md:items-end">
          <h2 className="max-w-xl font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Early, on purpose.
          </h2>
          <p className="max-w-sm text-[15px] leading-relaxed text-fg-muted">
            Vow is being designed in the open. There is no compiler to download yet; this is the
            order it gets built in.{" "}
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
          {PHASES.map((p, i) => (
            <li key={p.name} className="relative pl-9 md:pl-0">
              <span
                aria-hidden
                className={`absolute top-0 left-0 grid size-[15px] place-items-center rounded-full border ${
                  p.now ? "border-seal bg-seal" : "border-line-strong bg-bg"
                } md:relative`}
              >
                {p.now ? (
                  <span className="absolute inset-[-5px] rounded-full border border-seal/50 motion-safe:animate-[ping_2.4s_var(--ease-out)_infinite]" />
                ) : null}
              </span>
              <p className="font-mono text-[11.5px] text-fg-faint md:mt-5">
                {String(i).padStart(2, "0")}
                {p.now ? (
                  <span className="ml-2 rounded-full bg-seal-wash px-2 py-0.5 font-sans font-medium text-seal-text">
                    Now
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
