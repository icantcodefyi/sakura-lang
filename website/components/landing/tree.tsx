import { TreeLive } from "./tree-live";

/**
 * The hero's sakura tree: the poster (app/tree-*.svg, a CSS background so only the current
 * theme's file loads) at the reserved size, then the live scene over it where the device can
 * afford one. Decorative: the poster is a background and the canvas is aria-hidden.
 */
export function Tree({ className }: { className?: string }) {
  return (
    <TreeLive className={className}>
      {/* The poster is the hero's largest paint: fetch it with the HTML, not after the CSS that
          names it. The theme follows the system unless the visitor picked one. */}
      {(["light", "dark"] as const).map((s) => (
        <link
          key={s}
          rel="preload"
          as="image"
          href={`/tree-${s}.svg`}
          media={`(prefers-color-scheme: ${s})`}
          fetchPriority="high"
        />
      ))}
      <div className="tree-poster size-full" />
    </TreeLive>
  );
}
