import { notFound } from "next/navigation";
import { ogImage, quiet, sakuraInk } from "@/lib/og";
import { docsSection, source } from "@/lib/source";

export async function GET(_req: Request, { params }: RouteContext<"/og/docs/[...slug]">) {
  const { slug } = await params;
  const page = source.getPage(slug.slice(0, -1));
  if (!page) notFound();

  const section = docsSection(page.url);
  const { title, description = "" } = page.data;
  return ogImage(
    <div style={{ display: "flex", flexDirection: "column", maxWidth: 1000 }}>
      <div style={{ fontFamily: "Mono", fontSize: 26, color: sakuraInk }}>{section}</div>
      <div
        style={{
          marginTop: 14,
          fontFamily: "Display",
          fontSize: title.length > 24 ? 76 : 96,
          lineHeight: 1,
          letterSpacing: "-0.035em",
        }}
      >
        {title}
      </div>
      <div
        style={{
          marginTop: 28,
          fontFamily: "Body",
          fontSize: 30,
          lineHeight: 1.35,
          color: quiet,
          display: "block",
          lineClamp: 2,
        }}
      >
        {description}
      </div>
    </div>,
    {
      footer: page.url,
      // The ellipsis is for lineClamp.
      text: { Display: title, Body: `${description}…`, Mono: section },
    },
  );
}

export function generateStaticParams() {
  return source.getPages().map((page) => ({ slug: [...page.slugs, "image.png"] }));
}
