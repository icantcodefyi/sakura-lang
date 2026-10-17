import { docsLlms } from "@/lib/source";
import { site } from "@/lib/site";

export async function GET() {
  const body = `# Vow

> ${site.description}

Vow is in early design (pre-alpha). There is no compiler release yet; syntax may change.
Every docs page is available as Markdown: append \`.md\` to its URL (e.g. /docs/tour/effects.md).
All docs in one file: ${site.url}/llms-full.txt

${docsLlms.index()}`;
  return new Response(body, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
