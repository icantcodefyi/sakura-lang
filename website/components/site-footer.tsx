import { Mail } from "lucide-react";
import Link from "next/link";
import { Wordmark } from "@/components/brand";
import { GitHubIcon } from "@/components/github-icon";
import { LinkedInIcon, XIcon } from "@/components/social-icons";
import { RELEASE } from "@/lib/release";
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
      { href: "/changelog", label: "Changelog" },
      { href: "/docs/faq", label: "FAQ" },
      { href: site.github, label: "GitHub" },
    ],
  },
];

const SOCIAL = [
  { href: site.author.github, label: "GitHub", Icon: GitHubIcon },
  { href: site.author.x, label: "X", Icon: XIcon },
  { href: site.author.linkedin, label: "LinkedIn", Icon: LinkedInIcon },
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
            The language agents write. In beta: v{RELEASE} is published, not for production, and
            nothing is stable before 1.0.
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
      <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-x-6 gap-y-3 border-t border-border px-5 py-5 text-[13px] text-muted-foreground sm:px-8">
        <p>
          Made by{" "}
          <a
            href={site.author.github}
            rel="author me noopener"
            className="text-foreground underline decoration-sakura underline-offset-4 transition-colors duration-150 hover:text-sakura-text"
          >
            {site.author.name}
          </a>
          . Dual-licensed {site.license}.
        </p>
        <ul className="-mx-2 flex items-center">
          {SOCIAL.map(({ href, label, Icon }) => (
            <li key={label}>
              <a
                href={href}
                rel="me noopener"
                aria-label={`${site.author.name} on ${label}`}
                className="flex size-9 items-center justify-center rounded-md transition-colors duration-150 hover:text-foreground"
              >
                <Icon className="size-4" />
              </a>
            </li>
          ))}
          <li>
            <a
              href={`mailto:${site.author.email}`}
              aria-label={`Email ${site.author.name}`}
              className="flex size-9 items-center justify-center rounded-md transition-colors duration-150 hover:text-foreground"
            >
              <Mail className="size-4" aria-hidden />
            </a>
          </li>
        </ul>
      </div>
    </footer>
  );
}
