"use client";

import Link from "next/link";
import { ArrowUpRight, Menu } from "lucide-react";
import { Wordmark } from "@/components/brand";
import { Button } from "@/components/ui/button";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";
import { site } from "@/lib/site";

/** The phone menu: the same links as the desktop nav, in a shadcn Sheet (Radix Dialog). */
export function MobileNav({ links }: { links: { href: string; label: string }[] }) {
  return (
    <Sheet>
      <SheetTrigger asChild>
        <Button
          variant="ghost"
          size="icon-lg"
          className="rounded-full text-muted-foreground md:hidden"
          aria-label="Open menu"
        >
          <Menu strokeWidth={1.75} />
        </Button>
      </SheetTrigger>
      <SheetContent side="right" className="w-[min(20rem,86vw)] gap-0 p-0">
        <div className="flex h-14 items-center border-b border-border px-5">
          <SheetTitle className="text-[19px]">
            <Wordmark />
          </SheetTitle>
          <SheetDescription className="sr-only">Site navigation</SheetDescription>
        </div>
        <nav aria-label="Mobile" className="flex flex-col p-3">
          {links.map((l) => (
            <SheetClose asChild key={l.href}>
              <Link
                href={l.href}
                className="rounded-lg px-3 py-3 font-display text-[22px] font-semibold tracking-[-0.02em] transition-colors duration-150 hover:bg-muted"
              >
                {l.label}
              </Link>
            </SheetClose>
          ))}
        </nav>
        <div className="mt-auto flex flex-col gap-1 border-t border-border p-3 font-mono text-[13px] text-muted-foreground">
          {[
            { href: "/llms.txt", label: "/llms.txt" },
            { href: "/llms-full.txt", label: "/llms-full.txt" },
            { href: site.github, label: "GitHub" },
          ].map((l) => (
            <a
              key={l.href}
              href={l.href}
              className="flex items-center justify-between rounded-lg px-3 py-2.5 transition-colors duration-150 hover:bg-muted hover:text-foreground"
            >
              {l.label}
              <ArrowUpRight className="size-3.5" />
            </a>
          ))}
        </div>
      </SheetContent>
    </Sheet>
  );
}
