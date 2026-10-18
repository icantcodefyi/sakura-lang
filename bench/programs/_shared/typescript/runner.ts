// The same values/measure protocol as the native ports, with Bun's in-process clock.
export async function main(run: (n: number) => number | Promise<number>, maximum: number) {
  const [mode, ...args] = process.argv.slice(2);
  const stable = args.includes("--stable");
  const sizes = args.filter((arg) => arg !== "--stable").map(Number);
  if (!sizes.length || sizes.some((n) => !Number.isSafeInteger(n) || n < 0 || n > maximum)) {
    throw new Error("size outside oracle resource bound");
  }
  if (mode === "values") {
    const values: Record<string, string> = {};
    for (const n of sizes) values[n] = String(await run(n));
    process.stdout.write(`${JSON.stringify({ values })}\n`);
    return;
  }
  if (mode !== "measure" || sizes.length !== 1) throw new Error("measure <n> [--stable]");
  const n = sizes[0];
  let sink = 0;
  async function batch(count: number) {
    for (let i = 0; i < count; i++) {
      const value = run(n);
      sink = value instanceof Promise ? await value : value;
    }
  }
  const start = performance.now();
  while (performance.now() - start < (stable ? 2000 : 250)) await batch(1);
  let count = 1;
  for (;;) {
    const t = performance.now();
    await batch(count);
    if (performance.now() - t >= 30 || count >= 1 << 20) break;
    count *= 2;
  }
  for (let sample = 0; sample < 9; sample++) {
    const t = performance.now();
    await batch(count);
    process.stdout.write(`${JSON.stringify({ n, ms: (performance.now() - t) / count, checksum: String(sink) })}\n`);
  }
}
