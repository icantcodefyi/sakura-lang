import defaultMdxComponents from "fumadocs-ui/mdx";
import { Callout } from "fumadocs-ui/components/callout";
import { Step, Steps } from "fumadocs-ui/components/steps";
import { Tab, Tabs } from "fumadocs-ui/components/tabs";
import type { ComponentProps } from "react";
import { Play } from "lucide-react";
import type { MDXComponents } from "mdx/types";
import {
  AgentLoop,
  AuthorityFlow,
  CassetteFlow,
  ContractFlow,
  CountFlow,
  EffectFlow,
  FailureFlow,
  HandlerFlow,
  PatchFlow,
  PipelineFlow,
  VerifyFlow,
} from "@/components/docs-diagrams";
import {
  BenchChart,
  BenchRun,
  BenchSummary,
  BenchTable,
  BenchWins,
  NativeChart,
  NativeSummary,
} from "@/components/bench";

/** Callouts are tinted panels in the callout colour, without the default side stripe. */
function SakuraCallout({ className, ...props }: ComponentProps<typeof Callout>) {
  return (
    <Callout
      {...props}
      className={`sakura-callout ps-3.5 border-[color-mix(in_oklch,var(--callout-color)_35%,var(--border))] bg-[color-mix(in_oklch,var(--callout-color)_7%,var(--card))] [&>[role=none]]:hidden ${className ?? ""}`}
    />
  );
}

const DefaultPre = defaultMdxComponents.pre;

/** A code block, and below a whole program (source.config.ts marks it) a link to run it in /play. */
function Pre({ "data-play": play, ...props }: ComponentProps<"pre"> & { "data-play"?: string }) {
  if (!play) return <DefaultPre {...props} />;
  return (
    <>
      <DefaultPre {...props} />
      <p className="not-prose -mt-2 mb-4 flex justify-end">
        <a
          href={play}
          className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 font-mono text-[12px] text-fd-muted-foreground transition-colors hover:bg-fd-muted hover:text-fd-foreground"
        >
          <Play className="size-3 fill-current" aria-hidden />
          Open in playground
        </a>
      </p>
    </>
  );
}

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    AgentLoop,
    AuthorityFlow,
    BenchChart,
    BenchRun,
    BenchSummary,
    BenchTable,
    BenchWins,
    NativeChart,
    NativeSummary,
    Callout: SakuraCallout,
    pre: Pre,
    CassetteFlow,
    ContractFlow,
    CountFlow,
    EffectFlow,
    FailureFlow,
    HandlerFlow,
    PatchFlow,
    PipelineFlow,
    VerifyFlow,
    Step,
    Steps,
    Tab,
    Tabs,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
