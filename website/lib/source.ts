import { defineDocs } from "fumadocs-mdx/macro";
import { llms, loader } from "fumadocs-core/source";
import { renderPlaceholder } from "fumadocs-core/mdx-plugins/remark-llms.runtime";
import { site } from "@/lib/site";
import { diagramMarkdown } from "@/components/docs-diagrams";

const docs = defineDocs({
  dir: "content/docs",
  docs: {
    // From git history (vercel.json unshallows the clone): sitemap lastmod, JSON-LD dateModified.
    lastModified: true,
    postprocess: {
      // JSX blocks become placeholders, rendered to plain Markdown in renderPage below.
      includeProcessedMarkdown: {
        mdxAsPlaceholder: [
          "Callout",
          "Cards",
          "Card",
          "Steps",
          "Step",
          // docs diagrams (components/docs-diagrams.tsx), printed as Markdown by diagramMarkdown
          "AgentLoop",
          "CapabilityScopes",
          "CassetteFlow",
          "ContractFlow",
          "EffectFlow",
          "HandlerFlow",
          "PipelineFlow",
        ],
      },
    },
  },
});

export const source = loader({
  baseUrl: "/docs",
  source: docs.toFumadocsSource(),
});

const quote = (text: string) =>
  text
    .trim()
    .split("\n")
    .map((l) => (l ? `> ${l}` : ">"))
    .join("\n");

const markdownFor = {
  Callout: ({ attributes, children }: { attributes: Record<string, unknown>; children: string }) =>
    quote(`${attributes.title ? `**${attributes.title}**\n\n` : ""}${children}`),
  Cards: ({ children }: { children: string }) => children.trim(),
  Card: ({ attributes, children }: { attributes: Record<string, unknown>; children: string }) =>
    `- [${attributes.title}](${attributes.href})${children.trim() ? `: ${children.trim()}` : ""}`,
  Steps: ({ children }: { children: string }) => children.trim(),
  Step: ({ children }: { children: string }) => children.trim(),
  ...diagramMarkdown,
};

export const docsLlms = llms(source, {
  renderPage: async (page) => `# ${page.data.title}

URL: ${page.url}
Markdown: ${page.url === "/docs" ? "/docs.md" : `${page.url}.md`}
${page.data.description ? `\n> ${page.data.description}\n` : ""}
${await renderPlaceholder(await page.data.getText("processed"), markdownFor)}`,
});

/** The full /llms.txt body: a title block, then the docs index grouped by sidebar section. */
export function llmsTxt() {
  const index = docsLlms
    .index()
    .replace(/^# .*\n+/, "")
    .replace(/^- \*\*(.+)\*\*$/gm, "## $1")
    .replace(/\]\(\//g, `](${site.url}/`)
    .replace(/\n{3,}/g, "\n\n")
    .trim();

  return `# Sakura Lang

> ${site.description}

Sakura Lang is pre-alpha and nothing is released. The v0 language design is written (docs/design.md in the repository). The source-built compiler runs the core language and M3 standard library on JavaScript, including HTTP, SQLite, filesystem, CLI, AI and tasks. WASM build/run/test with a Node host and solver-backed verification are available as previews; backend parity and optimization work remain. Build it: ${site.url}/docs/quickstart

- The core language for agents, in one file: ${site.github}/blob/master/docs/llms.txt
- Every docs page is available as Markdown: append \`.md\` to its URL (for example ${site.url}/docs/tour/effects.md).
- All docs in one file: ${site.url}/llms-full.txt

${index}
`;
}

/** The sidebar section a docs page sits under (the nearest separator above it). */
export function docsSection(url: string) {
  let section = "Docs";
  for (const node of source.getPageTree().children) {
    if (node.type === "separator" && typeof node.name === "string") section = node.name;
    if (node.type === "page" && node.url === url) return section;
  }
  return "Docs";
}

/** The generated share card for a docs page (app/og/docs/[...slug]/route.tsx). */
export const docsImageUrl = (page: { slugs: string[] }) =>
  `/og/docs/${[...page.slugs, "image.png"].join("/")}`;
