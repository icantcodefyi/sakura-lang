"use client";

import dynamic from "next/dynamic";
import Link from "next/link";
import { Menu, Sun } from "lucide-react";
import { GitHubIcon } from "@/components/github-icon";
import { githubLinkClass } from "@/components/site-header";
import { site } from "@/lib/site";

/**
 * Same size and look as HeaderActions, without Radix. The menus and tooltip load after hydration
 * so their JavaScript stays off the critical path on phones (Lighthouse counts every script
 * requested before the largest paint).
 */
function ActionsPlaceholder() {
  return (
    <>
      <a href={site.github} className={githubLinkClass} aria-label="Sakura Lang on GitHub">
        <GitHubIcon className="size-[18px]" />
      </a>
      <span aria-hidden className="grid size-9 place-items-center text-muted-foreground">
        <Sun className="size-4" strokeWidth={1.75} />
      </span>
      <span aria-hidden className="grid size-9 place-items-center text-muted-foreground md:hidden">
        <Menu className="size-4" strokeWidth={1.75} />
      </span>
    </>
  );
}

const HeaderActions = dynamic(
  () => import("@/components/header-actions").then((m) => m.HeaderActions),
  { ssr: false, loading: ActionsPlaceholder },
);

/** The header's right side on full pages: llms.txt, GitHub, the theme menu and the phone menu. */
export function HeaderControls() {
  return (
    <>
      <Link
        href="/llms.txt"
        className="hidden rounded-full border border-border px-3 py-1 font-mono text-[12px] text-muted-foreground transition-colors duration-150 hover:border-strong hover:text-foreground sm:inline-block"
      >
        llms.txt
      </Link>
      <HeaderActions />
    </>
  );
}
