import { changelog } from "@/lib/changelog";

export function GET() {
  return new Response(changelog, {
    headers: { "Content-Type": "text/markdown; charset=utf-8" },
  });
}
