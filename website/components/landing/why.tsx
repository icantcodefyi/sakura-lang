import type { ReactNode } from "react";
import { tokenize } from "@/lib/sakura/highlight";
import { HeroDemo } from "./hero-demo";
import { RefineFigure } from "./refine-figure";
import { sample } from "./samples";
import { Section, TextLink } from "./section";
import { TestsFigure } from "./tests-figure";

const C = ({ children }: { children: ReactNode }) => (
  <code className="font-mono text-[0.92em] text-foreground">{children}</code>
);

function Reason({
  n,
  title,
  children,
  href,
  more,
}: {
  n: string;
  title: string;
  children: ReactNode;
  href: string;
  more: string;
}) {
  return (
    <div>
      <p className="font-mono text-[12px] text-faint tabular-nums">{n}</p>
      <h3 className="mt-2 font-display text-[1.35rem] leading-tight font-semibold tracking-[-0.02em] text-balance">
        {title}
      </h3>
      <p className="mt-3 max-w-[30rem] text-[15.5px] leading-relaxed text-pretty text-muted-foreground">
        {children}
      </p>
      <p className="mt-3 text-[14.5px]">
        <TextLink href={href}>{more}</TextLink>
      </p>
    </div>
  );
}

export async function Why() {
  const lines = await tokenize(sample("signup"));
  return (
    <Section
      id="why"
      ask="Why would I use it?"
      title="An agent can't quietly do more than the signature says."
      lede="Three checks you would otherwise do by hand in review, each run by the compiler on every change."
    >
      <div className="grid items-center gap-x-14 gap-y-10 border-t border-border pt-10 lg:grid-cols-[minmax(0,0.75fr)_minmax(0,1fr)]">
        <Reason
          n="01"
          title="Effects are part of the type."
          href="/docs/tour/effects"
          more="Effects"
        >
          <C>signup</C> may use the database, the clock and randomness, and fail with a{" "}
          <C>SignupError</C>; nothing else. When an agent adds a network call, <C>sakura check</C>{" "}
          stops it with <C>V3001</C>. Widening the signature is a fix marked <C>interface_change</C>
          , for a person to approve; here the agent removes the call instead.
        </Reason>
        <HeroDemo lines={lines} />
      </div>
      <div className="mt-16 grid gap-x-14 gap-y-16 border-t border-border pt-10 lg:grid-cols-2">
        <div className="grid content-start gap-8">
          <Reason
            n="02"
            title="Refinements are checked at compile time."
            href="/docs/tour/types"
            more="Refinement types"
          >
            <C>Cents</C> is an <C>Int</C> with a <C>where</C> clause. A literal that breaks it never
            compiles, and a plain <C>Int</C> can only become one through <C>Cents.from</C>, checked
            at run time, or <C>Cents.validate</C>, which returns a <C>Result</C>.
          </Reason>
          <RefineFigure />
        </div>
        <div className="grid content-start gap-8">
          <Reason
            n="03"
            title="Tests are deterministic by default."
            href="/docs/tour/testing"
            more="Testing"
          >
            Under <C>sakura test</C> the clock is fixed, randomness is seeded and output is
            captured. Any other effect needs a handler in the test, so a unit test can&apos;t reach
            the network by accident.
          </Reason>
          <TestsFigure />
        </div>
      </div>
    </Section>
  );
}
