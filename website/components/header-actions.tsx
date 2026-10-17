"use client";

import { GitHubIcon } from "@/components/github-icon";
import { MobileNav } from "@/components/mobile-nav";
import { githubLinkClass, navLinks } from "@/components/site-header";
import { ThemeToggle } from "@/components/theme-toggle";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { site } from "@/lib/site";

/** GitHub with its tooltip, the theme menu and the phone menu: the Radix-backed header controls. */
export function HeaderActions() {
  return (
    <>
      <TooltipProvider delayDuration={400}>
        <Tooltip>
          <TooltipTrigger asChild>
            <a href={site.github} className={githubLinkClass} aria-label="Sakura Lang on GitHub">
              <GitHubIcon className="size-[18px]" />
            </a>
          </TooltipTrigger>
          <TooltipContent sideOffset={6}>GitHub</TooltipContent>
        </Tooltip>
      </TooltipProvider>
      <ThemeToggle />
      <MobileNav links={navLinks} />
    </>
  );
}
