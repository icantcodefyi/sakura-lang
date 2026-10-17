import Link from "next/link";
import { Wordmark } from "@/components/brand";
import { GitHubIcon } from "@/components/github-icon";
import { MobileNav } from "@/components/mobile-nav";
import { ThemeToggle } from "@/components/theme-toggle";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { site } from "@/lib/site";

const links = [
  { href: "/docs", label: "Docs" },
  { href: "/docs/tour/basics", label: "Tour" },
  { href: "/docs/agents", label: "For agents" },
  { href: "/docs/roadmap", label: "Roadmap" },
];

export function SiteHeader() {
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
            {links.map((l) => (
              <Link
                key={l.href}
                href={l.href}
                className="rounded-full px-3 py-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground"
              >
                {l.label}
              </Link>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-1">
            <Link
              href="/llms.txt"
              className="hidden rounded-full border border-border px-3 py-1 font-mono text-[12px] text-muted-foreground transition-colors duration-150 hover:border-strong hover:text-foreground sm:inline-block"
            >
              llms.txt
            </Link>
            <Tooltip>
              <TooltipTrigger asChild>
                <a
                  href={site.github}
                  className="grid size-9 place-items-center rounded-full text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground"
                  aria-label="Sakura Lang on GitHub"
                >
                  <GitHubIcon className="size-[18px]" />
                </a>
              </TooltipTrigger>
              <TooltipContent sideOffset={6}>GitHub</TooltipContent>
            </Tooltip>
            <ThemeToggle />
            <MobileNav links={links} />
          </div>
        </div>
      </header>
    </>
  );
}
