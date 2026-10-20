import Link from "next/link";
import { ArrowUpRight } from "lucide-react";

/** Speed and the toolchain, side by side: what an agent waits on, and what it calls. */
export function Pillars() {
  return (
    <section id="toolchain" className="border-b border-border">
      <div className="mx-auto grid max-w-6xl px-5 py-24 sm:px-8 md:py-28 lg:grid-cols-2">
        <div className="pb-14 lg:pr-14 lg:pb-0">
          <Heading href="/docs/changes#analysis-cache-and-check-latency">
            Fast where an agent waits.
          </Heading>
          <p className="mt-4 max-w-[30rem] text-[16px] leading-relaxed text-pretty text-muted-foreground">
            An agent runs{" "}
            <code className="font-mono text-[15px] text-foreground">sakura check</code> after every
            edit, so check latency is the number it feels. The standard library&apos;s analysis is
            cached on disk per compiler build, and the LSP and MCP servers keep it in memory.
          </p>
          <Latency />
          <Backends />
          <WebStd />
        </div>
        <div className="border-t border-border pt-14 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-14">
          <Heading href="/docs/agents">One binary for the whole loop.</Heading>
          <p className="mt-4 max-w-[30rem] text-[16px] leading-relaxed text-pretty text-muted-foreground">
            Checker, verifier, runner, formatter, test runner, language server and MCP server are
            one compiler library behind one command, so every tool gives the same answer.
          </p>
          <div className="mt-8">
            <Toolchain />
          </div>
          <Editors />
        </div>
      </div>
    </section>
  );
}

function Heading({ href, children }: { href: string; children: string }) {
  return (
    <h2 className="font-display text-[clamp(1.5rem,2.4vw,1.9rem)] leading-tight font-semibold tracking-[-0.025em]">
      <Link href={href} className="group rounded-sm">
        {children.split(" ").slice(0, -1).join(" ")}{" "}
        <span className="whitespace-nowrap">
          {children.split(" ").at(-1)}
          <ArrowUpRight className="ml-1 inline-block size-5 align-[-2px] text-faint transition-[transform,color] duration-200 group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-sakura-text" />
        </span>
      </Link>
    </h2>
  );
}

// `bench/bench compile` workloads: `check_hello` on the `sakura new cli` project and
// `check_cold_10k` on its generated 10,084-line package; medians of repeated runs on the
// owner's reference machine. Runtime ratios stay off this page; the blessed instruction-count
// baseline lives on /docs/benchmarks (docs/release.md §2, design §17.264).
const LATENCY = [
  { ms: "7", what: "sakura check", on: "a new project" },
  { ms: "35", what: "sakura check", on: "a cold 10k-line package" },
];

function Latency() {
  return (
    <figure className="mt-8">
      <dl className="grid grid-cols-2 border-t border-foreground/80">
        {LATENCY.map((l, i) => (
          <div key={l.on} className={`py-5 ${i === 0 ? "pr-5" : "border-l border-border pl-5"}`}>
            <dt className="sr-only">
              {l.what}, {l.on}
            </dt>
            <dd>
              <span className="font-display text-[clamp(2.6rem,5vw,3.6rem)] leading-none font-semibold tracking-[-0.04em] tabular-nums">
                {l.ms}
                <span className="ml-1 text-[0.45em] tracking-normal text-muted-foreground">ms</span>
              </span>
              <span aria-hidden className="mt-2 block font-mono text-[12px] text-foreground">
                {l.what}
              </span>
              <span aria-hidden className="block text-[13px] text-muted-foreground">
                {l.on}
              </span>
            </dd>
          </div>
        ))}
      </dl>
      <figcaption className="border-t border-border pt-3 text-[12.5px] leading-relaxed text-faint">
        Measured on an Apple M4 Pro: release build, medians of repeated runs on the{" "}
        <code className="font-mono">bench/bench compile</code> workloads. Other machines will
        differ.
      </figcaption>
    </figure>
  );
}

const BACKENDS: { name: string; how: string; state: "ships" | "experimental" }[] = [
  { name: "JavaScript", how: "the default; runs on Node 26", state: "ships" },
  { name: "WebAssembly", how: "--target wasm, held to the same outputs", state: "ships" },
  { name: "Native", how: "Cranelift; in progress, not in any release", state: "experimental" },
];

function Backends() {
  return (
    <ul className="mt-8 grid border-t border-border">
      {BACKENDS.map((b) => (
        <li
          key={b.name}
          className="flex items-baseline justify-between gap-4 border-b border-border py-2.5"
        >
          <span className="text-[14.5px]">
            <span className="font-medium">{b.name}</span>{" "}
            <span className="text-muted-foreground">{b.how}</span>
          </span>
          <span
            className={`shrink-0 rounded-full border px-2 py-px font-mono text-[11px] ${
              b.state === "ships"
                ? "border-ok/45 bg-ok-wash text-ok"
                : "border-dashed border-strong text-muted-foreground"
            }`}
          >
            {b.state}
          </span>
        </li>
      ))}
    </ul>
  );
}

// The web-platform modules of CHANGELOG [0.1.0], Added (RFC 0002). Crypto on WASM is on main,
// not in v0.1.0, so its row says where it runs.
const WEB_STD = [
  ["bytes, encoding", "Base64, atob, TextEncoder, TextDecoder"],
  ["url, searchparams", "WHATWG URL and URLSearchParams"],
  ["headers, blob, form", "Headers, Blob, File, FormData, multipart"],
  ["crypto", "SHA-2, HMAC, HKDF, PBKDF2 on JavaScript"],
] as const;

function WebStd() {
  return (
    <div className="mt-8">
      <h3 className="font-mono text-[11.5px] text-faint">the web platform, as typed values</h3>
      <dl className="mt-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 border-t border-border text-[13.5px]">
        {WEB_STD.map(([mods, what]) => (
          <div key={mods} className="col-span-2 grid grid-cols-subgrid border-b border-border py-2">
            <dt className="font-mono text-[13px] whitespace-nowrap">
              <span className="text-faint">std/</span>
              {mods}
            </dt>
            <dd className="text-muted-foreground">{what}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

// `sakura --help` on main; every one ships in v0.1.0.
const COMMANDS = [
  ["check", "types, effects, exhaustiveness, holes"],
  ["fix", "applies fixes by impact class, never authority"],
  ["verify", "proves contracts with Z3, cross-checked by cvc5"],
  ["test", "deterministic handlers, properties, snapshots"],
  ["build", "JavaScript or WebAssembly, minus proved checks"],
  ["run", "builds and runs main under its grants"],
  ["fmt", "one canonical format"],
  ["audit", "trust, totality, refinement and external sites"],
  ["explain", "any code, with a wrong and a fixed program"],
  ["mcp serve", "the compiler as MCP tools"],
  ["lsp", "the language server editors talk to"],
  ["setup", "the skill and MCP entry for your agent"],
] as const;

function Toolchain() {
  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 border-t border-border font-mono text-[13px]">
      {COMMANDS.map(([cmd, what]) => (
        <div key={cmd} className="col-span-2 grid grid-cols-subgrid border-b border-border py-2">
          <dt className="whitespace-nowrap">
            <span className="text-faint">sakura </span>
            <span className="text-sakura-text">{cmd}</span>
          </dt>
          <dd className="font-sans text-[13.5px] text-muted-foreground">{what}</dd>
        </div>
      ))}
    </dl>
  );
}

function Editors() {
  return (
    <p className="mt-8 text-[14.5px] leading-relaxed text-muted-foreground">
      <span className="font-medium text-foreground">Editors.</span> A VS Code extension (also for
      Cursor and VSCodium) and a tree-sitter grammar for Neovim, Helix and Zed. Both build from the
      repository; neither is in a marketplace yet.{" "}
      <Link
        href="/docs/editors"
        className="font-medium text-foreground underline decoration-sakura underline-offset-4"
      >
        Editor setup
      </Link>
    </p>
  );
}
