import type { Metadata } from "next";
import { HeaderControls } from "@/components/header-controls";
import { Playground } from "@/components/play/playground";
import { SiteHeader } from "@/components/site-header";
import { homeImageAlt } from "@/lib/og";
import { examples } from "@/lib/play-examples";
import { tokenize } from "@/lib/sakura/highlight";
import { pageMetadata } from "@/lib/seo";

export const metadata: Metadata = pageMetadata({
  title: "Playground",
  description:
    "Write Sakura Lang and run it in your browser: the compiler runs as WebAssembly and the program in a sandboxed worker, with nothing to install.",
  path: "/play",
  image: { url: "/opengraph-image", alt: homeImageAlt },
});

export default async function PlayPage() {
  // The first example arrives highlighted, so the editor does not change once the highlighter loads.
  const initialTokens = await tokenize(examples[0]?.code ?? "");
  return (
    <>
      <SiteHeader>
        <HeaderControls />
      </SiteHeader>
      <main id="main" className="flex flex-1 flex-col">
        <h1 className="sr-only">Sakura Lang playground</h1>
        <Playground examples={examples} initialTokens={initialTokens} />
      </main>
    </>
  );
}
