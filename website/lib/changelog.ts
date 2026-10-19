import "server-only";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** The repository CHANGELOG.md (vercel.json redeploys the site when it changes). */
export const changelog = readFileSync(join(process.cwd(), "..", "CHANGELOG.md"), "utf8");

const SECTION = /^## \[(.+?)\](?: - (.+))?$/gm;
const anchor = (version: string) => (version === "Unreleased" ? "unreleased" : `v${version}`);

// The `## [version] - date` sections without the file's title and preamble; an empty section
// (Unreleased right after a release) is dropped.
const sections = changelog
  .slice(changelog.indexOf("\n## ") + 1)
  .replace(/^## .*\n\s*(?=^## )/gm, "");

/** The sections in file order (newest first), for the page's release index. */
export const releases = [...sections.matchAll(SECTION)].map(([, version, date]) => ({
  version,
  date,
  id: anchor(version),
}));

/** The sections, each heading with a `#v0.1.0`-style id (remarkHeading's custom id syntax). */
export const changelogEntries = sections.replace(
  SECTION,
  (line, version: string) => `${line} [#${anchor(version)}]`,
);
