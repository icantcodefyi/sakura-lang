import Link from "next/link";

// Commands and platform rows from /docs/install (content/docs/install.mdx) and
// packaging/targets.json. Update both together when a release publishes.
const VERSION = "0.1.0-alpha.1";

const CHANNELS: { name: string; lines: string[]; note: string }[] = [
  {
    name: "npm",
    lines: [`npm install --global sakuralang@${VERSION}`],
    note: "the npm latest tag; needs Node 26",
  },
  {
    name: "Homebrew",
    lines: ["brew tap icantcodefyi/sakuralang", "brew install sakuralang"],
    note: "installs Node with it",
  },
  {
    name: "install.sh",
    lines: [
      `curl -fsSLO https://github.com/icantcodefyi/sakuralang/releases/download/v${VERSION}/install.sh`,
      `sh install.sh --version ${VERSION}`,
    ],
    note: "checks SHA256SUMS; verifies attestations when gh is installed",
  },
];

type Stage = "published" | "v0.1.0" | "v0.2.0";

const PLATFORMS: { name: string; target: string; stage: Stage; note?: string }[] = [
  { name: "macOS, Apple silicon", target: "aarch64-apple-darwin", stage: "published" },
  {
    name: "Linux x64",
    target: "x86_64-unknown-linux-gnu",
    stage: "published",
    note: "glibc 2.35+",
  },
  {
    name: "Linux ARM64",
    target: "aarch64-unknown-linux-gnu",
    stage: "v0.1.0",
    note: "Raspberry Pi 4 and 5 expected to work",
  },
  { name: "Windows x64", target: "x86_64-pc-windows-msvc", stage: "v0.2.0" },
  { name: "macOS, Intel", target: "x86_64-apple-darwin", stage: "v0.2.0" },
];

const STAGE: Record<Stage, { label: string; className: string }> = {
  published: { label: "in alpha.1", className: "border-ok/45 bg-ok-wash text-ok" },
  "v0.1.0": {
    label: "coming in v0.1.0",
    className: "border-sakura/55 bg-sakura-wash text-sakura-text",
  },
  "v0.2.0": {
    label: "coming in v0.2.0",
    className: "border-dashed border-strong text-muted-foreground",
  },
};

export function Install() {
  return (
    <section id="install" className="border-b border-border">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8 md:py-32">
        <div className="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,26rem)] md:items-end md:gap-16">
          <h2 className="font-display text-[clamp(2rem,3.6vw,2.9rem)] leading-[1.04] font-semibold tracking-[-0.03em] text-balance">
            Install it today.
          </h2>
          <p className="text-[17px] leading-relaxed text-pretty text-muted-foreground">
            <code className="font-mono text-[15px] text-foreground">sakuralang@{VERSION}</code> is a
            preview with no compatibility promise yet. <code className="font-mono">check</code> and{" "}
            <code className="font-mono">build</code> need nothing else;{" "}
            <code className="font-mono">run</code> and <code className="font-mono">test</code> need
            Node 26.
          </p>
        </div>

        <div className="mt-12 grid gap-x-16 gap-y-12 lg:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)]">
          <ul className="grid border-t border-foreground/80">
            {CHANNELS.map((c) => (
              <li key={c.name} className="grid gap-2 border-b border-border py-5">
                <p className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                  <span className="font-display text-[1.05rem] font-semibold tracking-[-0.01em]">
                    {c.name}
                  </span>
                  <span className="text-[13px] text-muted-foreground">{c.note}</span>
                </p>
                <pre className="font-mono text-[12.5px] leading-[1.7] text-foreground">
                  {c.lines.map((l) => (
                    <code
                      key={l}
                      className="block pl-[2ch] -indent-[2ch] break-all whitespace-pre-wrap select-all"
                    >
                      <span aria-hidden className="text-faint select-none">
                        ${" "}
                      </span>
                      {l}
                    </code>
                  ))}
                </pre>
              </li>
            ))}
          </ul>

          <div>
            <table className="w-full border-collapse text-left">
              <caption className="pb-3 text-left font-mono text-[11.5px] text-faint">
                platforms
              </caption>
              <tbody className="border-t border-foreground/80">
                {PLATFORMS.map((p) => (
                  <tr key={p.target} className="border-b border-border">
                    <th scope="row" className="py-3.5 pr-4 align-top font-normal">
                      <span className="block text-[14.5px] font-medium">{p.name}</span>
                      <span className="block font-mono text-[11.5px] text-faint">{p.target}</span>
                      {p.note ? (
                        <span className="block text-[12.5px] text-muted-foreground">{p.note}</span>
                      ) : null}
                    </th>
                    <td className="py-3.5 text-right align-top">
                      <span
                        className={`inline-block rounded-full border px-2.5 py-0.5 font-mono text-[11.5px] whitespace-nowrap ${STAGE[p.stage].className}`}
                      >
                        {STAGE[p.stage].label}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="mt-4 text-[13.5px] leading-relaxed text-muted-foreground">
              No platform is called <span className="font-medium text-foreground">Supported</span>{" "}
              until a release&apos;s published-install smoke passes on a hosted runner of that OS
              and CPU.{" "}
              <Link
                href="/docs/install#platform-claims"
                className="font-medium text-foreground underline decoration-sakura underline-offset-4"
              >
                Claim levels
              </Link>
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
