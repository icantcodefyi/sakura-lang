import defaultMdxComponents from "fumadocs-ui/mdx";
import { Callout } from "fumadocs-ui/components/callout";
import { Step, Steps } from "fumadocs-ui/components/steps";
import { Tab, Tabs } from "fumadocs-ui/components/tabs";
import type { ComponentProps } from "react";
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
import { BenchChart, BenchRun, BenchTable } from "@/components/bench";

/** Callouts are tinted panels in the callout colour, without the default side stripe. */
function SakuraCallout({ className, ...props }: ComponentProps<typeof Callout>) {
  return (
    <Callout
      {...props}
      className={`sakura-callout ps-3.5 border-[color-mix(in_oklch,var(--callout-color)_35%,var(--border))] bg-[color-mix(in_oklch,var(--callout-color)_7%,var(--card))] [&>[role=none]]:hidden ${className ?? ""}`}
    />
  );
}

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    AgentLoop,
    AuthorityFlow,
    BenchChart,
    BenchRun,
    BenchTable,
    Callout: SakuraCallout,
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
