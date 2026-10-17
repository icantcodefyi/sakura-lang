import type { Metadata } from "next";
import Link from "next/link";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { Button } from "@/components/ui/button";

// Next adds `noindex` to every not-found response.
export const metadata: Metadata = { title: "Page not found" };

export default function NotFound() {
  return (
    <>
      <SiteHeader />
      <main
        id="main"
        className="mx-auto flex w-full max-w-6xl flex-1 flex-col justify-center px-5 py-28 sm:px-8"
      >
        <p className="font-mono text-[13px] text-fail">404: no page at this path</p>
        <h1 className="mt-4 font-display text-[clamp(2.4rem,5vw,3.6rem)] leading-[1.02] font-semibold tracking-[-0.03em]">
          This page never made that promise.
        </h1>
        <p className="mt-5 max-w-[34rem] text-[17px] leading-relaxed text-muted-foreground">
          The link may be out of date, or the page has not been written yet.
        </p>
        <div className="mt-8 flex flex-wrap gap-3">
          <Button asChild size="lg">
            <Link href="/docs">Read the docs</Link>
          </Button>
          <Button asChild size="lg" variant="outline">
            <Link href="/">Home</Link>
          </Button>
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
