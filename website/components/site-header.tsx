import type { ReactNode } from "react";
import Link from "next/link";
import { Wordmark } from "@/components/brand";

export const navLinks = [
  { href: "/docs", label: "Docs" },
  { href: "/docs/tour/basics", label: "Tour" },
  { href: "/docs/agents", label: "For agents" },
  // Not prefetched: its client code would load on every page that shows the header.
  { href: "/play", label: "Playground", prefetch: false },
  { href: "/docs/roadmap", label: "Roadmap" },
];

export const githubLinkClass =
  "grid size-9 place-items-center rounded-full text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground";

/**
 * The site bar. Interactive controls come in as `children` (see HeaderControls) so that pages
 * without them, like the root 404 whose client code ships with every route, stay light.
 */
export function SiteHeader({ children }: { children?: ReactNode }) {
  return (
    <>
      <a
        href="#main"
        className="fixed top-2 left-2 z-50 -translate-y-16 rounded-full bg-foreground px-4 py-2 text-[14px] font-medium text-background transition-transform duration-150 focus-visible:translate-y-0"
      >
        Skip to content
      </a>
      <header className="sticky top-0 z-40 border-b border-border/70 bg-background/80 backdrop-blur-xl backdrop-saturate-150 supports-[not(backdrop-filter:blur(1px))]:bg-background">
        <div className="mx-auto flex h-14 max-w-6xl items-center gap-6 px-5 sm:px-8">
          <Link href="/" aria-label="Sakura Lang home" className="rounded-md text-[19px]">
            <Wordmark />
          </Link>
          <nav aria-label="Primary" className="hidden items-center gap-1 text-[14px] md:flex">
            {navLinks.map((l) => (
              <Link
                key={l.href}
                href={l.href}
                prefetch={"prefetch" in l ? l.prefetch : undefined}
                className="rounded-full px-3 py-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground"
              >
                {l.label}
              </Link>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-1">{children}</div>
        </div>
      </header>
    </>
  );
}
