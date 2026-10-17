import defaultMdxComponents from "fumadocs-ui/mdx";
import { Callout } from "fumadocs-ui/components/callout";
import { Step, Steps } from "fumadocs-ui/components/steps";
import { Tab, Tabs } from "fumadocs-ui/components/tabs";
import type { ComponentProps } from "react";
import type { MDXComponents } from "mdx/types";

/** Callouts are tinted panels in the callout colour, without the default side stripe. */
function VowCallout({ className, ...props }: ComponentProps<typeof Callout>) {
  return (
    <Callout
      {...props}
      className={`vow-callout ps-3.5 border-[color-mix(in_oklch,var(--callout-color)_35%,var(--line))] bg-[color-mix(in_oklch,var(--callout-color)_7%,var(--surface))] [&>[role=none]]:hidden ${className ?? ""}`}
    />
  );
}

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    Callout: VowCallout,
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
