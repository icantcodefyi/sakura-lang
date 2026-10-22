import "server-only";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

export type Example = { name: string; label: string; description: string; code: string };

const dir = join(process.cwd(), "..", "compiler", "examples");

/** Services the browser host refuses (components/play/playground.tsx) and runs that need input. */
const UNRUNNABLE = /^\/\/ args:|^import std\/(http|serve|sql|llm|websocket|worker)\b/m;
/** The fixture harness's header directives (compiler/examples/README.md). */
const DIRECTIVES = /^(\/\/ (milestone|needs|mode|covers|stdout-regex|spec|known-bug):.*\n)+\n?/;

/** What the picker shows for each example; a new example falls back to its directory name. */
const ABOUT: Record<string, [label: string, description: string]> = {
  hello: ["Hello", "Print a greeting with the time"],
  contracts: ["Contracts", "Refined types, preconditions and postconditions"],
  counter: ["Counter effect", "Your own effect with a stateful handler"],
  graph: ["Graph", "Friends of friends through node IDs"],
  indexed_update: ["List updates", "Replace items with List.set"],
  json_refined: ["JSON decoding", "Decode into a record that checks its fields"],
  typed_errors: ["Typed errors", "Translate failures into a stable error type"],
  url_build: ["URLs", "Build and resolve URLs with query parameters"],
};

/**
 * The repository's runnable examples that need nothing the playground lacks: `mode: run`, no
 * arguments, no network, database or model services. `hello` first, then by name.
 */
export const examples: Example[] = readdirSync(dir, { withFileTypes: true })
  .filter((e) => e.isDirectory())
  .flatMap((e) => {
    let code: string;
    try {
      code = readFileSync(join(dir, e.name, "src", "main.sak"), "utf8");
    } catch {
      return [];
    }
    if (!/^\/\/ mode: run$/m.test(code) || UNRUNNABLE.test(code)) return [];
    const [label, description] = ABOUT[e.name] ?? [e.name.replaceAll("_", " "), ""];
    return [{ name: e.name, label, description, code: code.replace(DIRECTIVES, "") }];
  })
  .toSorted(
    (a, b) =>
      Number(b.name === "hello") - Number(a.name === "hello") || a.name.localeCompare(b.name),
  );
