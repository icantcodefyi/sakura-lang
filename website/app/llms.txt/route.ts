import { llmsTxt } from "@/lib/source";

export async function GET() {
  return new Response(llmsTxt(), { headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
