type Tree = { l: Tree; r: Tree } | null;
// A leaf is null; a node is a two-field object.
function bottomUp(depth: number): Tree {
  return depth > 0 ? { l: bottomUp(depth - 1), r: bottomUp(depth - 1) } : { l: null, r: null };
}
function check(t: Tree): number {
  return t === null ? 0 : 1 + check(t.l) + check(t.r);
}
export function run(n: number) {
  const minDepth = 4;
  const maxDepth = Math.max(minDepth + 2, n);
  let total = check(bottomUp(maxDepth + 1));
  const longLived = bottomUp(maxDepth);
  for (let d = minDepth; d <= maxDepth; d += 2) {
    const iterations = 2 ** (maxDepth - d + minDepth);
    for (let i = 0; i < iterations; i++) total += check(bottomUp(d));
  }
  return total + check(longLived);
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 14);
