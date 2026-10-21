import { Mail } from "lucide-react";
import Link from "next/link";
import type { CSSProperties } from "react";
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

/** A loose petal, about 1 unit tall, in the footer art's own shape (public/footer-branch-*.svg). */
const PETAL =
  "M0 .42L.07 .48L.14 .53L.2 .51L.25 .44L.29 .36L.32 .28L.33 .18L.33 .09L.32 0L.29-.08L.26-.16L.23-.23L.19-.28L.15-.32L.11-.36L.07-.38L.04-.4L0-.4L-.04-.4L-.07-.38L-.11-.36L-.15-.32L-.19-.28L-.23-.23L-.26-.16L-.29-.08L-.32 0L-.33 .09L-.33 .18L-.32 .28L-.29 .36L-.25 .44L-.2 .51L-.14 .53L-.07 .48Z";

/** Petals resting on the wordmark, in the art's coordinates: x, y, length, width, turn. */
const RESTING = [
  [269, 197, 16, 7.2, 40],
  [200, 231, 13, 5.85, 130],
] as const;

/** Falling petals: left edge, drift, fall time, phase and size. */
const FALLING = [
  ["54.6%", "-79px", "22.1s", "-13.1s", 1.18],
  ["15%", "-87px", "22.5s", "-13.6s", 0.98],
  ["82.2%", "-98px", "21.8s", "-14.6s", 1.09],
  ["23.7%", "-112px", "17.9s", "-7.1s", 0.94],
  ["19.2%", "-80px", "23s", "-13.3s", 0.96],
  ["78.9%", "-63px", "26.1s", "-8.3s", 1.06],
  ["12.9%", "-66px", "19s", "-18.6s", 0.9],
] as const;

export function SiteFooter() {
  return (
    <footer className="site-footer relative overflow-hidden border-t border-border">
      {/* Shapes for the petals below; each theme sets the --petal-* stops (globals.css). */}
      <svg width="0" height="0" className="absolute" aria-hidden="true">
        <defs>
          <path id="ft-petal" d={PETAL} />
          <linearGradient id="ft-petal-rest" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="var(--petal-rest-a)" />
            <stop offset=".45" stopColor="var(--petal-rest-b)" />
            <stop offset="1" stopColor="var(--petal-rest-c)" />
          </linearGradient>
          <linearGradient id="ft-petal-fall" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="var(--petal-fall-a)" />
            <stop offset="1" stopColor="var(--petal-fall-b)" />
          </linearGradient>
        </defs>
      </svg>
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
      {/*
        The name, set huge and quiet, with a flowering limb of the landing tree reaching in from
        the right. The box keeps the art's shape, so nothing shifts as it loads, and the name is
        sized in the box's own width so it keeps its place under the branch. The name sits in
        front of the limb, and the art leaves the "L" of Lang clear; two petals rest on it.
      */}
      <div className="footer-scene" aria-hidden="true">
        <div className="footer-branch" />
        <p className="footer-name">Sakura Lang</p>
        <svg viewBox="0 40 1180 300" className="absolute inset-0 size-full">
          {RESTING.map(([x, y, l, w, a]) => (
            <g key={x}>
              <use
                href="#ft-petal"
                fill="var(--petal-shadow)"
                transform={`translate(${x + 1} ${y + 2}) scale(${l} ${w}) rotate(${a})`}
              />
              <use
                href="#ft-petal"
                fill="url(#ft-petal-rest)"
                transform={`translate(${x} ${y}) scale(${l} ${w}) rotate(${a})`}
              />
            </g>
          ))}
        </svg>
      </div>
      <div className="footer-fall" aria-hidden="true">
        {FALLING.map(([x, dx, t, d, s]) => (
          <i
            key={x}
            style={{ "--x": x, "--dx": dx, "--t": t, "--d": d, "--s": s } as CSSProperties}
          >
            <svg viewBox="-.55 -.55 1.1 1.1">
              <use href="#ft-petal" fill="url(#ft-petal-fall)" />
            </svg>
          </i>
        ))}
      </div>
    </footer>
  );
}
