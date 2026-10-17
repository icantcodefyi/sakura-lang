import { docsLlms } from "@/lib/source";
import { site } from "@/lib/site";

export async function GET() {
  const index = docsLlms
    .index()
    .replace(/^# .*\n+/, "") // we write our own title block
    .replace(/^- \*\*(.+)\*\*$/gm, "## $1") // sidebar separators become sections
    .replace(/\]\(\//g, `](${site.url}/`)
    .replace(/\n{3,}/g, "\n\n")
    .trim();

  const body = `# Vow

> ${site.description}

Vow is in early design (pre-alpha). There is no compiler release yet, and syntax may change.

- Every docs page is available as Markdown: append \`.md\` to its URL (for example ${site.url}/docs/tour/effects.md).
- All docs in one file: ${site.url}/llms-full.txt

${index}
`;
  return new Response(body, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
