import type { Metadata } from "next";
import { SiteFooter } from "@/components/site-footer";
import { HeaderControls } from "@/components/header-controls";
import { SiteHeader } from "@/components/site-header";
import { Agents } from "@/components/landing/agents";
import { Benchmarks } from "@/components/landing/benchmarks";
import { CodeSample } from "@/components/landing/code";
import { Hero } from "@/components/landing/hero";
import { Install } from "@/components/landing/install";
import { Status } from "@/components/landing/status";
import { Why } from "@/components/landing/why";
import { homeImageAlt } from "@/lib/og";
import { absolute, author, ids, JsonLd, pageMetadata } from "@/lib/seo";
import { RELEASE } from "@/lib/release";
import { site } from "@/lib/site";

export const metadata: Metadata = {
  // A page-level openGraph drops the layout's file-based image, so name it again.
  ...pageMetadata({
    title: site.title,
    description: site.description,
    path: "/",
    image: { url: "/opengraph-image", alt: homeImageAlt },
  }),
  title: { absolute: site.title },
};

const jsonLd = {
  "@graph": [
    author,
    {
      "@type": "WebSite",
      "@id": ids.website,
      url: absolute("/"),
      name: site.name,
      description: site.description,
      inLanguage: "en",
      publisher: { "@id": ids.author },
      about: { "@id": ids.language },
    },
    {
      "@type": "SoftwareSourceCode",
      "@id": ids.language,
      name: site.name,
      description: site.description,
      url: absolute("/"),
      codeRepository: site.github,
      programmingLanguage: "Rust",
      license: ["https://spdx.org/licenses/MIT.html", "https://spdx.org/licenses/Apache-2.0.html"],
      creativeWorkStatus: "Preview",
      version: RELEASE,
      author: { "@id": ids.author },
      maintainer: { "@id": ids.author },
      image: absolute("/opengraph-image"),
      sameAs: [site.github],
    },
  ],
};

export default function Home() {
  return (
    <>
      <SiteHeader>
        <HeaderControls />
      </SiteHeader>
      <main id="main" className="flex-1">
        {/* The questions a curious developer asks, in order: what, why, how it works with an
            agent, how it reads, how fast,
            how ready, how to start. Everything else lives in the docs. */}
        <Hero />
        <Why />
        <Agents />
        <CodeSample />
        <Benchmarks />
        <Status />
        <Install />
      </main>
      <SiteFooter />
      <JsonLd data={jsonLd} />
    </>
  );
}
