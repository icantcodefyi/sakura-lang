import Link from "next/link";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";

export default function NotFound() {
  return (
    <>
      <SiteHeader />
      <main
        id="main"
        className="mx-auto flex w-full max-w-6xl flex-1 flex-col justify-center px-5 py-28 sm:px-8"
      >
        <p className="font-mono text-[13px] text-fail">error E0404: no page at this path</p>
        <h1 className="mt-4 font-display text-[clamp(2.4rem,5vw,3.6rem)] leading-[1.02] font-semibold tracking-[-0.03em]">
          This page never made that promise.
        </h1>
        <p className="mt-5 max-w-[34rem] text-[17px] leading-relaxed text-fg-muted">
          The link may be out of date, or the page has not been written yet.
        </p>
        <div className="mt-8 flex flex-wrap gap-3 text-[15px]">
          <Link
            href="/docs"
            className="rounded-full bg-seal px-5 py-2.5 font-semibold text-seal-ink"
          >
            Read the docs
          </Link>
          <Link href="/" className="rounded-full border border-line-strong px-5 py-2.5 font-medium">
            Home
          </Link>
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
