import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * A landing-page code sample: the part of `samples/<name>.sak` above its `// @landing-context`
 * line. `scripts/check-doc-sweep.py` runs `sakura check` on the whole file, so what the page
 * shows always compiles (or fails exactly as the marker says) against the current compiler.
 */
export function sample(name: string): string {
  const file = readFileSync(
    join(process.cwd(), "components/landing/samples", `${name}.sak`),
    "utf8",
  );
  return file.split("\n// @landing-context")[0];
}
