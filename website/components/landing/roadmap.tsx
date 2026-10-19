import Link from "next/link";

// The release stages of docs/plans/road-to-1.0.md. When a user-visible milestone lands, move
// its `status` along here and in the hero badge (website/AGENTS.md). No dates: the plan has none.
const PHASES: { id: string; name: string; body: string; status?: "shipped" | "now" }[] = [
  {
    id: "v0.1.0-alpha.1",
    name: "The preview",
    body: "M0 to M3: the language, std, LSP, MCP, the skill, WASM and the verify preview, on npm, Homebrew and install.sh.",
    status: "shipped",
  },
  {
    id: "v0.1.0",
    name: "M4 exit",
    body: "The first non-alpha preview, after one quiet benchmark window. Adds Linux ARM64.",
    status: "now",
  },
  {
    id: "v0.2.0",
    name: "Native and stability",
    body: "Native backend behind an experimental flag, Windows x64 and Intel Mac, and the stability guarantees of RFC 0001.",
  },
  {
    id: "v1.0.0",
    name: "Stable",
    body: "A release candidate, a seven-day soak with no P1 defect, then 1.0. Packages follow in 1.x.",
  },
];

export function Roadmap() {
  return (
    <section id="roadmap" className="border-b border-border">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="flex flex-col justify-between gap-6 md:flex-row md:items-end">
          <h2 className="max-w-xl font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Shipping in the open.
          </h2>
          <p className="max-w-sm text-[15px] leading-relaxed text-muted-foreground">
            alpha.1 is out. The rest of the road to 1.0 is weeks of work, not dated: each step ships
            when its evidence is in.{" "}
            <Link
              href="/docs/roadmap"
              className="font-medium text-foreground underline decoration-sakura underline-offset-4"
            >
              Full roadmap
            </Link>
          </p>
        </div>

        <ol className="relative mt-14 grid gap-8 md:grid-cols-2 lg:grid-cols-4 lg:gap-8">
          <span
            aria-hidden
            className="absolute top-[7px] right-0 left-0 hidden h-px bg-strong lg:block"
          />
          <span
            aria-hidden
            className="absolute top-0 bottom-0 left-[7px] w-px bg-strong md:hidden"
          />
          {PHASES.map((p) => (
            <li key={p.id} className="relative pl-9 md:pl-0">
              <span
                aria-hidden
                className={`absolute top-0 left-0 grid size-[15px] place-items-center rounded-full border ${
                  p.status === "now"
                    ? "border-sakura bg-sakura"
                    : p.status === "shipped"
                      ? "border-ok bg-ok text-background"
                      : "border-strong bg-background"
                } md:relative`}
              >
                {p.status === "now" ? (
                  <span className="absolute inset-[-5px] rounded-full border border-sakura/50 motion-safe:animate-[ping_2.4s_var(--ease-out)_infinite]" />
                ) : p.status === "shipped" ? (
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
              <p className="font-mono text-[11.5px] text-faint md:mt-5">
                {p.id}
                {p.status === "now" ? (
                  <span className="ml-2 rounded-full bg-sakura-wash px-2 py-0.5 font-sans font-medium text-sakura-text">
                    Next
                  </span>
                ) : p.status === "shipped" ? (
                  <span className="ml-2 rounded-full bg-ok-wash px-2 py-0.5 font-sans font-medium text-ok">
                    Shipped
                  </span>
                ) : null}
              </p>
              <h3 className="mt-2 font-display text-[1.15rem] font-semibold tracking-[-0.015em]">
                {p.name}
              </h3>
              <p className="mt-1.5 text-[14px] leading-relaxed text-muted-foreground">{p.body}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}
