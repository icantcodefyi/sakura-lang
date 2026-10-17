import Link from "next/link";
import { Wordmark } from "@/components/brand";
import { GitHubIcon } from "@/components/github-icon";
import { ThemeToggle } from "@/components/theme-toggle";
import { site } from "@/lib/site";

const links = [
  { href: "/docs", label: "Docs" },
  { href: "/docs/tour/basics", label: "Tour" },
  { href: "/docs/agents", label: "For agents" },
  { href: "/docs/roadmap", label: "Roadmap" },
];

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-40 border-b border-line/70 bg-bg/80 backdrop-blur-xl backdrop-saturate-150 supports-[not(backdrop-filter:blur(1px))]:bg-bg">
      <div className="mx-auto flex h-14 max-w-6xl items-center gap-6 px-5 sm:px-8">
        <Link href="/" aria-label="Vow home" className="rounded-md text-[19px]">
          <Wordmark />
        </Link>
        <nav aria-label="Primary" className="hidden items-center gap-1 text-[14px] md:flex">
          {links.map((l) => (
            <Link
              key={l.href}
              href={l.href}
              className="rounded-full px-3 py-1.5 text-fg-muted transition-colors duration-150 hover:bg-bg-sunk hover:text-fg"
            >
              {l.label}
            </Link>
          ))}
        </nav>
        <div className="ml-auto flex items-center gap-1">
          <Link
            href="/llms.txt"
            className="hidden rounded-full border border-line px-3 py-1 font-mono text-[12px] text-fg-muted transition-colors duration-150 hover:border-line-strong hover:text-fg sm:inline-block"
          >
            llms.txt
          </Link>
          <a
            href={site.github}
            className="grid size-9 place-items-center rounded-full text-fg-muted transition-colors duration-150 hover:bg-bg-sunk hover:text-fg"
            aria-label="Vow on GitHub"
          >
            <GitHubIcon className="size-[18px]" />
          </a>
          <ThemeToggle />
          <Link
            href="/docs"
            className="ml-1 rounded-full bg-fg px-3.5 py-1.5 text-[13.5px] font-medium text-bg transition-transform duration-150 active:scale-[0.97] md:hidden"
          >
            Docs
          </Link>
        </div>
      </div>
    </header>
  );
}
