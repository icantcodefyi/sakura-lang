import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";
import { Wordmark } from "@/components/brand";
import { site } from "@/lib/site";

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: <Wordmark className="text-lg" />,
      url: "/",
    },
    githubUrl: site.github,
    links: [
      { text: "Docs", url: "/docs", active: "nested-url" },
      { text: "llms.txt", url: "/llms.txt", external: true },
    ],
  };
}
