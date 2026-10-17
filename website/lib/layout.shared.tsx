import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";
import { Wordmark } from "@/components/brand";
import { GitHubIcon } from "@/components/github-icon";
import { site } from "@/lib/site";

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: <Wordmark className="text-lg" />,
      url: "/",
    },
    links: [
      // Not `githubUrl`: its built-in icon is an unlabelled role="img" svg (fails svg-img-alt).
      {
        type: "icon",
        url: site.github,
        text: "GitHub",
        label: "GitHub",
        icon: <GitHubIcon />,
        external: true,
      },
      { text: "llms.txt", url: "/llms.txt", external: true },
      { text: "llms-full.txt", url: "/llms-full.txt", external: true },
    ],
  };
}
