import { site } from "@/lib/site";

// The release line of docs/plans/road-to-1.0.md, in order.
const LINE = ["0.1.0-alpha.1", "0.1.0", "1.0.0-beta.1", "1.0.0-beta.2", "1.0.0"] as const;
export type Version = (typeof LINE)[number];

/**
 * The newest published release, the one source for every version on the site. Change it only
 * once the tag is published (docs/release.md); the hero, install rows, release strip, footer
 * and llms.txt follow.
 */
export const RELEASE: Version = "1.0.0-beta.2";

/** Short display name: `alpha.1` for a prerelease, `v0.1.0` otherwise. */
export const releaseLabel = (v: Version = RELEASE) =>
  v.includes("-") ? v.slice(v.indexOf("-") + 1) : `v${v}`;

/** Whether `v` is published (at or before RELEASE on the line). */
export const isReleased = (v: Version) => LINE.indexOf(v) <= LINE.indexOf(RELEASE);

export const releaseUrl = (v: Version = RELEASE) => `${site.github}/releases/tag/v${v}`;
