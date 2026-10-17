import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { AgentView } from "@/components/landing/agent-view";
import { Closing } from "@/components/landing/closing";
import { Compare } from "@/components/landing/compare";
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
        <Review />
        <Pillars />
        <AgentView />
        <Compare />
        <Roadmap />
        <Closing />
      </main>
      <SiteFooter />
    </>
  );
}
