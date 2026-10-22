import { RELEASE } from "@/lib/release";
import { PipelineFigure } from "./pipeline-figure";
import { Section, TextLink } from "./section";

// docs/stability ("By area"): what RFC 0001 holds stable across 1.x, and what stays
// experimental.
const STABLE = [
  "CLI commands, flags and exit codes",
  "Diagnostic codes and --json records",
  "Standard library signatures",
  "WebAssembly runtime interface",
  "Editor (LSP) and agent (MCP) tools",
];
const EXPERIMENTAL = ["--target native", "[dependencies] packages", "std/worker and @worker"];

export function Status() {
  return (
    <Section
      id="status"
      ask="Is it ready?"
      title="1.0 is out."
      lede={
        <>
          <code className="font-mono text-[0.92em] text-foreground">v{RELEASE}</code> is the first
          stable release. The language, standard library, CLI, JSON output and runtime ABI keep
          working across 1.x; breaking changes wait for 2.0. Native, packages and workers are
          experimental.
        </>
      }
    >
      <div className="grid gap-x-16 gap-y-12 lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] lg:items-center">
        <PipelineFigure />
        <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-1">
          <div>
            <h3 className="font-mono text-[12px] text-faint">stable in 1.x</h3>
            <ul className="mt-3 grid border-t border-border">
              {STABLE.map((s) => (
                <li
                  key={s}
                  className="flex items-center gap-3 border-b border-border py-2 text-[14.5px]"
                >
                  <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-ok" />
                  {s}
                </li>
              ))}
            </ul>
          </div>
          <div>
            <h3 className="font-mono text-[12px] text-faint">experimental</h3>
            <ul className="mt-3 grid border-t border-border">
              {EXPERIMENTAL.map((s) => (
                <li
                  key={s}
                  className="flex items-center gap-3 border-b border-border py-2 font-mono text-[13.5px]"
                >
                  <span
                    aria-hidden
                    className="size-1.5 shrink-0 rounded-full border border-dashed border-faint"
                  />
                  {s}
                </li>
              ))}
            </ul>
            <p className="mt-4 flex flex-wrap gap-x-5 gap-y-1 text-[14.5px]">
              <TextLink href="/docs/stability">Stability</TextLink>
              <TextLink href="/docs/roadmap">Roadmap</TextLink>
            </p>
          </div>
        </div>
      </div>
    </Section>
  );
}
