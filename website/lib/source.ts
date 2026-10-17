import { defineDocs } from "fumadocs-mdx/macro";
import { llms, loader } from "fumadocs-core/source";

const docs = defineDocs({
  dir: "content/docs",
  docs: {
    postprocess: {
      includeProcessedMarkdown: true,
    },
  },
});

export const source = loader({
  baseUrl: "/docs",
  source: docs.toFumadocsSource(),
});

export const docsLlms = llms(source, {
  renderPage: async (page) => `# ${page.data.title}

URL: ${page.url}
Markdown: ${page.url === "/docs" ? "/docs.md" : `${page.url}.md`}
${page.data.description ? `\n> ${page.data.description}\n` : ""}
${await page.data.getText("processed")}`,
});
