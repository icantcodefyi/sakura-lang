import { Seal } from "@/components/seal";
import { RELEASE as VERSION } from "@/lib/release";
import { CopyCommand } from "./copy-command";
import { Play } from "./play";
import { Section, TextLink } from "./section";
import { Tabs } from "./tabs";

// Commands from /docs/install (content/docs/install.mdx); platforms and claim levels live there.
const CHANNELS: { name: string; lines: string[]; note: string }[] = [
  { name: "npm", lines: ["npm i -g sakuralang"], note: "needs Node 26" },
  {
    name: "Homebrew",
    lines: [
      "brew tap icantcodefyi/sakuralang",
      "brew trust --tap icantcodefyi/sakuralang",
      "brew install sakuralang",
    ],
    note: "installs Node with it",
  },
  {
    name: "install.sh",
    lines: [
      `curl -fsSLO \\\n  https://github.com/icantcodefyi/sakuralang/releases/download/v${VERSION}/install.sh`,
      `sh install.sh --version ${VERSION}`,
    ],
    note: "checks SHA256SUMS; verifies attestations when gh is installed",
  },
];

// `sakura new cli hello`, then the three commands, as run on main: each prints the line beside it.
const FIRST = [
  ["sakura new cli hello && cd hello", "scaffolds src/main.sak, AGENTS.md, CLAUDE.md"],
  ["sakura check", "no problems"],
  ["sakura test", "1 passed, 0 failed, 0 skipped (seed 0)"],
  ["sakura run", "hello, world"],
];

export function Install() {
  return (
    <Section
      id="install"
      ask="How do I start?"
      title="Install it, then check a project."
      lede={
        <>
          Binaries for macOS, Linux and Windows. <TextLink href="/docs/install">Platforms</TextLink>{" "}
          <span className="text-faint">·</span>{" "}
          <TextLink href="/docs/quickstart">Quickstart</TextLink>{" "}
          <span className="text-faint">·</span> <TextLink href="/docs/agents">For agents</TextLink>
        </>
      }
    >
      <div className="grid gap-x-16 gap-y-12 lg:grid-cols-2">
        <Tabs
          label="Install channels"
          items={CHANNELS.map((c) => ({
            label: c.name,
            panel: (
              <div>
                <CopyCommand commands={c.lines} />
                <p className="mt-2 text-[13px] text-muted-foreground">{c.note}</p>
              </div>
            ),
          }))}
        />
        <div className="relative">
          <ol className="grid border-t border-foreground/80 font-mono text-[13px]">
            {FIRST.map(([cmd, out], i) => (
              <li
                key={cmd}
                className="grid grid-cols-[1.5rem_minmax(0,1fr)] gap-x-3 border-b border-border py-3"
              >
                <span className="text-faint tabular-nums">{i + 1}</span>
                <span className="min-w-0">
                  <span className="block text-foreground">
                    <span aria-hidden className="text-faint select-none">
                      ${" "}
                    </span>
                    {cmd}
                  </span>
                  <span className="mt-0.5 block font-sans text-[13px] text-muted-foreground">
                    {out}
                  </span>
                </span>
              </li>
            ))}
          </ol>
          {/* Pressed once, when the steps scroll into view. */}
          <Play className="pointer-events-none absolute -top-12 right-0 hidden size-28 rotate-[-14deg] sm:block">
            <Seal bloom className="size-full" />
          </Play>
        </div>
      </div>
    </Section>
  );
}
