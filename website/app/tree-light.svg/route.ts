import { posterSvg, svgResponse } from "@/lib/tree-poster";

export function GET() {
  return svgResponse(posterSvg("light"));
}
