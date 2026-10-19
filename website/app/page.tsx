import type { Metadata } from "next";
import { SiteFooter } from "@/components/site-footer";
import { HeaderControls } from "@/components/header-controls";
import { SiteHeader } from "@/components/site-header";
import { AgentView } from "@/components/landing/agent-view";
import { Anatomy } from "@/components/landing/anatomy";
import { Authority } from "@/components/landing/authority";
import { Closing } from "@/components/landing/closing";
import { Compare } from "@/components/landing/compare";
import { Effects } from "@/components/landing/effects";
import { Guarantees } from "@/components/landing/guarantees";
import { Hero } from "@/components/landing/hero";
import { Install } from "@/components/landing/install";
import { Pillars } from "@/components/landing/pillars";
import { Review } from "@/components/landing/review";
import { Roadmap } from "@/components/landing/roadmap";
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
        <Hero />
        <Install />
        <Anatomy />
        <Review />
        <Effects />
        <Authority />
        <Guarantees />
        <AgentView />
        <Pillars />
        <Compare />
        <Roadmap />
        <Closing />
      </main>
      <SiteFooter />
      <JsonLd data={jsonLd} />
    </>
  );
}
