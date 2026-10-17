import Link from "next/link";
import { Wordmark } from "@/components/brand";
import { site } from "@/lib/site";

const COLUMNS = [
  {
    title: "Learn",
    links: [
      { href: "/docs", label: "Introduction" },
      { href: "/docs/why-sakura-lang", label: "Why Sakura Lang" },
      { href: "/docs/tour/basics", label: "Language tour" },
      { href: "/docs/stdlib", label: "Standard library" },
    ],
  },
  {
    title: "For agents",
    links: [
      { href: "/llms.txt", label: "llms.txt" },
      { href: "/llms-full.txt", label: "llms-full.txt" },
      { href: "/docs/agents/mcp", label: "MCP tools" },
      { href: "/docs/agents/diagnostics", label: "Diagnostics" },
    ],
  },
  {
    title: "Project",
    links: [
      { href: "/docs/roadmap", label: "Roadmap" },
      { href: "/docs/faq", label: "FAQ" },
      { href: site.github, label: "GitHub" },
    ],
  },
];

export function SiteFooter() {
  return (
    <footer className="border-t border-border">
      <div className="mx-auto grid max-w-6xl gap-10 px-5 py-14 sm:px-8 md:grid-cols-[1.4fr_repeat(3,1fr)]">
        <div>
          <Link href="/" aria-label="Sakura Lang home" className="text-[19px]">
            <Wordmark />
          </Link>
          <p className="mt-4 max-w-[17rem] text-[14px] leading-relaxed text-muted-foreground">
            The language agents write. Pre-alpha: the compiler is under construction and nothing is
            released.
          </p>
        </div>
        {COLUMNS.map((c) => (
          <nav key={c.title} aria-label={c.title}>
            <h2 className="text-[13px] font-medium text-faint">{c.title}</h2>
            <ul className="mt-3 grid gap-2 text-[14px]">
              {c.links.map((l) => (
                <li key={l.href}>
                  <Link
                    href={l.href}
                    className="text-muted-foreground transition-colors duration-150 hover:text-foreground"
                  >
                    {l.label}
                  </Link>
                </li>
              ))}
            </ul>
          </nav>
        ))}
      </div>
    </footer>
  );
}
