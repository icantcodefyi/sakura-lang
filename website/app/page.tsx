import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { AgentView } from "@/components/landing/agent-view";
import { Anatomy } from "@/components/landing/anatomy";
import { Authority } from "@/components/landing/authority";
import { Closing } from "@/components/landing/closing";
import { Compare } from "@/components/landing/compare";
import { Effects } from "@/components/landing/effects";
import { Guarantees } from "@/components/landing/guarantees";
import { Hero } from "@/components/landing/hero";
import { Pillars } from "@/components/landing/pillars";
import { Review } from "@/components/landing/review";
import { Roadmap } from "@/components/landing/roadmap";

export default function Home() {
  return (
    <>
      <SiteHeader />
      <main id="main" className="flex-1">
        <Hero />
        <Anatomy />
        <Review />
        <Effects />
        <Authority />
        <Guarantees />
        <AgentView />
        <Pillars />
        <Compare />
        <Roadmap />
        <Closing />
      </main>
      <SiteFooter />
    </>
  );
}
