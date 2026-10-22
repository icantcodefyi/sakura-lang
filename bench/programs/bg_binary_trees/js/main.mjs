// binary-trees, the Benchmarks Game's task: the stretch tree, the long-lived tree and the
// bottom-up trees per depth, with the suite's report lines. Every node is a two-field object,
// leaves included (their children are null); the engine's default GC. Same algorithm as the
// Sakura program. run(n) is the digest of the output bytes (bench/README.md); output(n) is the
// output as base64, as Sakura's is.
function bottomUp(depth) {
  return depth > 0 ? { l: bottomUp(depth - 1), r: bottomUp(depth - 1) } : { l: null, r: null };
}

function check(t) {
  return t === null ? 0 : 1 + check(t.l) + check(t.r);
}

function text(n) {
  const minDepth = 4;
  const maxDepth = Math.max(minDepth + 2, n);
  const stretchDepth = maxDepth + 1;
  let out = `stretch tree of depth ${stretchDepth}\t check: ${check(bottomUp(stretchDepth))}\n`;
  const longLived = bottomUp(maxDepth);
  for (let d = minDepth; d <= maxDepth; d += 2) {
    const iterations = 2 ** (maxDepth - d + minDepth);
    let sum = 0;
    for (let i = 0; i < iterations; i++) sum += check(bottomUp(d));
    out += `${iterations}\t trees of depth ${d}\t check: ${sum}\n`;
  }
  return `${out}long lived tree of depth ${maxDepth}\t check: ${check(longLived)}\n`;
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
function digest(bytes) {
  let h = 0;
  for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) % 1000000007;
  return h;
}

const encoder = new TextEncoder();
export function run(n) {
  return digest(encoder.encode(text(n)));
}

export function output(n) {
  return Buffer.from(text(n)).toString("base64");
}
