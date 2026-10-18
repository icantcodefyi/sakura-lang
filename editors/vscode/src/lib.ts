// The parts of the client that need no VS Code API, so `bun test` covers them directly.

import { accessSync, constants, statSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, isAbsolute, join, resolve } from "node:path";

export const INSTALL_URL = "https://www.sakuralang.com/docs/quickstart#install-from-source";

/** The nearest directory at or above `file` that holds a `sakura.toml`, or `undefined`. */
export function projectRoot(file: string): string | undefined {
  let dir = dirname(resolve(file));
  for (;;) {
    if (isFile(join(dir, "sakura.toml"))) return dir;
    const parent = dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
}

export type Executable = { path: string } | { error: string };

/**
 * The `sakura` to start: the `sakura.path` setting when it is set (a leading `~/` is the home
 * directory, a relative path is taken from `base`), else the first `sakura` on `PATH`.
 */
export function findExecutable(
  setting: string,
  env: NodeJS.ProcessEnv = process.env,
  base = process.cwd(),
  platform: NodeJS.Platform = process.platform,
): Executable {
  const configured = setting.trim();
  if (configured !== "") {
    const path = configured.startsWith("~/")
      ? join(homedir(), configured.slice(2))
      : resolve(base, configured);
    if (isExecutable(path, platform)) return { path };
    return {
      error: `The \`sakura.path\` setting points at ${path}, which is not an executable file.`,
    };
  }
  const names = platform === "win32" ? ["sakura.exe", "sakura.cmd", "sakura"] : ["sakura"];
  const dirs = (env.PATH ?? env.Path ?? "").split(delimiter);
  for (const dir of dirs) {
    if (dir === "" || !isAbsolute(dir)) continue;
    for (const name of names) {
      const path = join(dir, name);
      if (isExecutable(path, platform)) return { path };
    }
  }
  return { error: "No `sakura` executable was found on PATH and `sakura.path` is not set." };
}

/** A quick fix's title with its impact class (design §12.1), e.g. `… (impact: behavior change)`. */
export function titleWithImpact(title: string, data: unknown): string {
  const impact = (data as { impact?: unknown } | undefined)?.impact;
  if (typeof impact !== "string") return title;
  return `${title} (impact: ${impact.replaceAll("_", " ")})`;
}

function isFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function isExecutable(path: string, platform: NodeJS.Platform): boolean {
  if (!isFile(path)) return false;
  if (platform === "win32") return true;
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}
