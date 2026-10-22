import "server-only";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

export type Example = { name: string; code: string };

const dir = join(process.cwd(), "..", "compiler", "examples");

/** Services the browser host refuses (components/play/playground.tsx) and runs that need input. */
const UNRUNNABLE = /^\/\/ args:|^import std\/(http|serve|sql|llm|websocket|worker)\b/m;
/** The fixture harness's header directives (compiler/examples/README.md). */
const DIRECTIVES = /^(\/\/ (milestone|needs|mode|covers|stdout-regex|spec|known-bug):.*\n)+\n?/;

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
    return [{ name: e.name, code: code.replace(DIRECTIVES, "") }];
  })
  .toSorted(
    (a, b) =>
      Number(b.name === "hello") - Number(a.name === "hello") || a.name.localeCompare(b.name),
  );
