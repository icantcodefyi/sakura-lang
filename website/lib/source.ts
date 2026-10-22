import { defineDocs } from "fumadocs-mdx/macro";
import { llms, loader } from "fumadocs-core/source";
import { renderPlaceholder } from "fumadocs-core/mdx-plugins/remark-llms.runtime";
import { RELEASE } from "@/lib/release";
import { site } from "@/lib/site";
import { diagramMarkdown } from "@/components/docs-diagrams";
import {
  chartMarkdown,
  runMarkdown,
  summaryMarkdown,
  tableMarkdown,
  tables,
  winsMarkdown,
  type TableName,
} from "@/lib/bench";

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
          // the benchmarks page (components/bench.tsx), printed from lib/bench.ts
          "BenchChart",
          "BenchRun",
          "BenchSummary",
          "BenchTable",
          "BenchWins",
          // docs diagrams (components/docs-diagrams.tsx), printed as Markdown by diagramMarkdown
          "AgentLoop",
          "AuthorityFlow",
          "CassetteFlow",
          "ContractFlow",
          "CountFlow",
          "EffectFlow",
          "FailureFlow",
          "HandlerFlow",
          "PatchFlow",
          "PipelineFlow",
          "VerifyFlow",
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
  BenchChart: chartMarkdown,
  BenchRun: runMarkdown,
  BenchSummary: summaryMarkdown,
  BenchWins: winsMarkdown,
  BenchTable: ({ attributes }: { attributes: Record<string, unknown> }) =>
    tableMarkdown(tables[attributes.table as TableName]),
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

Sakura Lang is in beta: the latest release is v${RELEASE}, not for production, with no stable language or API contract before 1.0. The docs track main; changes since 0.1.0: ${site.url}/docs/changes. The v0 language design is written (docs/design.md in the repository). The compiler runs the whole language and the standard library on JavaScript and WebAssembly, \`sakura verify\` proves contracts with Z3, and native code generation is experimental and in no release. Install it: ${site.url}/docs/install

- Portable Sakura skill: ${site.url}/skills/sakura/SKILL.md
- Skill ZIP: ${site.url}/skills/sakura-skill.zip (checksum: ${site.url}/skills/sakura-skill.sha256; provenance and file hashes: ${site.url}/skills/manifest.json). Extract the sakura/ folder into \`.claude/skills/\`, \`.agents/skills/\`, or \`.cursor/skills/\` in your project.
- The core language for agents, in one file: ${site.github}/blob/main/docs/llms.txt
- Every docs page is available as Markdown: append \`.md\` to its URL (for example ${site.url}/docs/tour/effects.md).
- All docs in one file: ${site.url}/llms-full.txt
- Changelog: ${site.url}/changelog.md

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
