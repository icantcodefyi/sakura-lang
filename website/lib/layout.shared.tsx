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
      { text: "llms.txt", url: "/llms.txt", external: true },
      { text: "llms-full.txt", url: "/llms-full.txt", external: true },
    ],
  };
}
