export const docsContentRoute = "/llms.mdx/docs";

/** Public, agent-friendly Markdown URL for a docs page (`/docs/foo.md`). */
export function getPageMarkdownUrl(page: { slugs: string[] }) {
  return page.slugs.length === 0 ? "/docs.md" : `/docs/${page.slugs.join("/")}.md`;
}
