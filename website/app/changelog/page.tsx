import type { ComponentProps } from "react";
import type { Metadata } from "next";
import Link from "next/link";
import { Markdown } from "fumadocs-core/content/md";
import { remarkGfm, remarkHeading } from "fumadocs-core/mdx-plugins";
import { HeaderControls } from "@/components/header-controls";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { changelogEntries, releases } from "@/lib/changelog";
import { homeImageAlt } from "@/lib/og";
import { RELEASE, releaseUrl } from "@/lib/release";
import { pageMetadata } from "@/lib/seo";
import { site } from "@/lib/site";

const description = "The notable changes to Sakura Lang, from CHANGELOG.md in the repository.";

export const metadata: Metadata = pageMetadata({
  title: "Changelog",
  description,
  path: "/changelog",
  image: { url: "/opengraph-image", alt: homeImageAlt },
  alternates: { "text/markdown": "/changelog.md" },
});

/** CHANGELOG.md links repository paths relatively; send them to the file on GitHub. */
function RepoLink({ href = "", children, ...props }: ComponentProps<"a">) {
  const url = /^([a-z]+:|#|\/)/.test(href) ? href : `${site.github}/blob/main/${href}`;
  return (
    <a href={url} {...props}>
      {children}
    </a>
  );
}

const link =
  "text-foreground underline decoration-sakura decoration-[1.5px] underline-offset-4 transition-colors duration-150 hover:text-sakura-text";

export default function Changelog() {
  return (
    <>
      <SiteHeader>
        <HeaderControls />
      </SiteHeader>
      <main id="main" className="mx-auto w-full max-w-6xl flex-1 px-5 pt-16 pb-24 sm:px-8 md:pt-24">
        <p className="font-mono text-[12px] tracking-[0.14em] text-sakura-text uppercase sm:text-[12.5px]">
          Changelog
        </p>
        <h1 className="mt-3 font-display text-[clamp(2.4rem,5vw,3.6rem)] leading-[1.02] font-semibold tracking-[-0.03em] text-balance">
          What changed on the way to 1.0.
        </h1>
        <p className="mt-5 max-w-[38rem] text-[17px] leading-relaxed text-pretty text-muted-foreground">
          {description} A section marked Unreleased is on main and not tagged yet.
        </p>
        <dl className="mt-8 grid max-w-[38rem] grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 border-t border-dotted border-foreground/25 pt-4 font-mono text-[12.5px] text-muted-foreground">
          <dt className="text-foreground">releases</dt>
          <dd className="flex flex-wrap gap-x-3">
            {releases.map((r) => (
              <span key={r.id}>
                <a href={`#${r.id}`} className={link}>
                  {r.version}
                </a>
                {r.date ? <span className="text-faint"> {r.date}</span> : null}
              </span>
            ))}
          </dd>
          <dt className="text-foreground">latest</dt>
          <dd>
            <a href={releaseUrl()} className={link}>
              v{RELEASE}
            </a>{" "}
            on GitHub
          </dd>
          <dt className="text-foreground">upgrading</dt>
          <dd>
            <Link href="/docs/changes" className={link}>
              /docs/changes
            </Link>
            , migration notes for main
          </dd>
          <dt className="text-foreground">agents</dt>
          <dd>
            GET{" "}
            {/* oxlint-disable-next-line nextjs/no-html-link-for-pages -- plain-text route, not a page */}
            <a href="/changelog.md" className={link}>
              /changelog.md
            </a>
          </dd>
        </dl>

        <article className="prose mt-14 max-w-[760px] border-t border-foreground/80 pt-2 [&_:is(h2,h3)]:scroll-mt-20">
          <Markdown remarkPlugins={[remarkGfm, remarkHeading]} components={{ a: RepoLink }}>
            {changelogEntries}
          </Markdown>
        </article>
      </main>
      <SiteFooter />
    </>
  );
}
