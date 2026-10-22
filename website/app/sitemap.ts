import type { MetadataRoute } from "next";
import { absolute } from "@/lib/seo";
import { source } from "@/lib/source";

export default function sitemap(): MetadataRoute.Sitemap {
  const pages = source.getPages();
  const latest = Math.max(...pages.map((p) => p.data.lastModified?.getTime() ?? 0));
  return [
    {
      url: absolute("/"),
      lastModified: latest ? new Date(latest) : undefined,
      changeFrequency: "weekly",
      priority: 1,
    },
    { url: absolute("/changelog"), changeFrequency: "weekly", priority: 0.6 },
    { url: absolute("/play"), changeFrequency: "weekly", priority: 0.6 },
    ...pages.map((page) => ({
      url: absolute(page.url),
      lastModified: page.data.lastModified,
      changeFrequency: "weekly" as const,
      priority: page.url === "/docs" ? 0.9 : 0.7,
    })),
  ];
}
