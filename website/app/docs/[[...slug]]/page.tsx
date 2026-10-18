import type { Metadata } from "next";
import { notFound } from "next/navigation";
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  EditOnGitHub,
  MarkdownCopyButton,
  ViewOptionsPopover,
} from "fumadocs-ui/layouts/docs/page";
import { createRelativeLink } from "fumadocs-ui/mdx";
import { getMDXComponents } from "@/components/mdx";
import { absolute, author, ids, JsonLd, pageMetadata } from "@/lib/seo";
import { getPageMarkdownUrl } from "@/lib/shared";
import { site } from "@/lib/site";
import { docsImageUrl, docsSection, source } from "@/lib/source";

export default async function Page(props: PageProps<"/docs/[[...slug]]">) {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const MDX = page.data.body;
  const markdownUrl = getPageMarkdownUrl(page);
  const githubUrl = `${site.github}/blob/main/website/content/docs/${page.path}`;
  const url = absolute(page.url);
  const crumbs = [
    { name: site.name, url: absolute("/") },
    { name: "Docs", url: absolute("/docs") },
    ...(page.url === "/docs" ? [] : [{ name: page.data.title, url }]),
  ];

  return (
    // 760px keeps body lines near 75 characters at the 17px prose size.
    <DocsPage
      toc={page.data.toc}
      full={page.data.full}
      className={page.data.full ? undefined : "max-w-[760px]"}
    >
      <DocsTitle className="font-display text-[clamp(2rem,3vw,2.5rem)] leading-[1.08] tracking-[-0.03em]">
        {page.data.title}
      </DocsTitle>
      <DocsDescription>{page.data.description}</DocsDescription>
      <div className="-mt-2 flex flex-row flex-wrap items-center gap-2 border-b border-fd-border pb-6">
        <MarkdownCopyButton markdownUrl={markdownUrl}>Copy as Markdown</MarkdownCopyButton>
        <ViewOptionsPopover
          markdownUrl={markdownUrl}
          pageUrl={new URL(page.url, site.url).toString()}
          githubUrl={githubUrl}
        />
      </div>
      <DocsBody>
        <MDX components={getMDXComponents({ a: createRelativeLink(source, page) })} />
      </DocsBody>
      <EditOnGitHub href={githubUrl} className="mt-6 self-start" />
      <JsonLd
        data={{
          "@graph": [
            {
              "@type": "TechArticle",
              "@id": `${url}#article`,
              headline: page.data.title,
              description: page.data.description,
              url,
              mainEntityOfPage: url,
              image: absolute(docsImageUrl(page)),
              inLanguage: "en",
              articleSection: docsSection(page.url),
              dateModified: page.data.lastModified?.toISOString(),
              author,
              publisher: { "@id": ids.author },
              isPartOf: { "@id": ids.website },
              about: { "@id": ids.language },
            },
            {
              "@type": "BreadcrumbList",
              itemListElement: crumbs.map((c, i) => ({
                "@type": "ListItem",
                position: i + 1,
                name: c.name,
                item: c.url,
              })),
            },
          ],
        }}
      />
    </DocsPage>
  );
}

export function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata(props: PageProps<"/docs/[[...slug]]">): Promise<Metadata> {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const { description = site.description, lastModified } = page.data;
  const section = docsSection(page.url);
  // A section's index page is titled "Overview" in the sidebar; name the section in search results.
  const title =
    page.slugs.length > 0 && page.path.endsWith("index.mdx")
      ? `${section}: ${page.data.title}`
      : page.data.title;
  return pageMetadata({
    title,
    description,
    path: page.url,
    image: { url: docsImageUrl(page), alt: `${page.data.title}: ${section}, Sakura Lang docs` },
    alternates: { "text/markdown": getPageMarkdownUrl(page) },
    openGraph: {
      type: "article",
      section,
      modifiedTime: lastModified?.toISOString(),
      authors: [site.author.github],
    },
  });
}
