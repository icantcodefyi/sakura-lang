import { defineDocs } from "fumadocs-mdx/macro";
import { llms, loader } from "fumadocs-core/source";
import { renderPlaceholder } from "fumadocs-core/mdx-plugins/remark-llms.runtime";
import { site } from "@/lib/site";

const docs = defineDocs({
  dir: "content/docs",
  docs: {
    postprocess: {
      // JSX blocks become placeholders, rendered to plain Markdown in renderPage below.
      includeProcessedMarkdown: { mdxAsPlaceholder: ["Callout", "Cards", "Card", "Steps", "Step"] },
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

  return `# Vow

> ${site.description}

Vow is in early design (pre-alpha). There is no compiler release yet, and syntax may change.

- Every docs page is available as Markdown: append \`.md\` to its URL (for example ${site.url}/docs/tour/effects.md).
- All docs in one file: ${site.url}/llms-full.txt

${index}
`;
}
