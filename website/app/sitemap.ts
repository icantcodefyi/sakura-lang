import type { MetadataRoute } from "next";
import { site } from "@/lib/site";
import { source } from "@/lib/source";

const url = (path: string) => new URL(path, site.url).toString();

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: url("/"), changeFrequency: "weekly", priority: 1 },
    ...source.getPages().map((page) => ({
      url: url(page.url),
      changeFrequency: "weekly" as const,
      priority: page.url === "/docs" ? 0.9 : 0.7,
    })),
  ];
}
