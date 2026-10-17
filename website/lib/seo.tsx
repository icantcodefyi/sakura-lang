import type { Metadata } from "next";
import { site } from "@/lib/site";

export const absolute = (path: string) => new URL(path, site.url).toString();

type OpenGraph = NonNullable<Metadata["openGraph"]>;

/**
 * Canonical, Open Graph and Twitter metadata for one page. Next replaces (not merges) a parent's
 * `openGraph`, `twitter` and `alternates`, so every page restates the site-wide fields here.
 * Without `image`, the nearest opengraph-image file applies.
 */
export function pageMetadata({
  title,
  description,
  path,
  image,
  openGraph,
  alternates,
}: {
  title: string;
  description: string;
  path: string;
  image?: { url: string; alt: string };
  openGraph?: Partial<OpenGraph>;
  alternates?: Record<string, string>;
}): Metadata {
  const images = image && [{ ...image, width: 1200, height: 630 }];
  return {
    title,
    description,
    alternates: {
      canonical: path,
      types: { "text/plain": [{ url: "/llms.txt", title: "llms.txt" }], ...alternates },
    },
    openGraph: {
      type: "website",
      siteName: site.name,
      locale: "en_US",
      url: path,
      title,
      description,
      images,
      ...openGraph,
    } as OpenGraph,
    twitter: {
      card: "summary_large_image",
      site: site.author.handle,
      creator: site.author.handle,
      title,
      description,
      images,
    },
  };
}

export const ids = {
  author: absolute("/#author"),
  website: absolute("/#website"),
  language: absolute("/#language"),
};

export const author = {
  "@type": "Person",
  "@id": ids.author,
  name: site.author.name,
  url: site.author.github,
  email: `mailto:${site.author.email}`,
  sameAs: [site.author.github, site.author.x, site.author.linkedin],
};

export function JsonLd({ data }: { data: Record<string, unknown> }) {
  return (
    <script
      type="application/ld+json"
      // Escape `<` so page text can never close the script element.
      dangerouslySetInnerHTML={{
        __html: JSON.stringify({ "@context": "https://schema.org", ...data }).replace(
          /</g,
          "\\u003c",
        ),
      }}
    />
  );
}
