// Profile warm kernel loops separately from timed measurements; print consumed checksum.
const [entry, size, seconds = '3'] = process.argv.slice(2);
const mod = await import(entry);
const n = Number(size);
const call = () => {
  const result = mod.runTask(mod.program.run, [n]);
  if (result.kind !== 'ok') throw new Error(JSON.stringify(result));
  return result.value;
};
let checksum, calls = 0;
const end = performance.now() + Number(seconds) * 1000;
while (performance.now() < end) { checksum = call(); calls++; }
console.log(JSON.stringify({n, calls, checksum}));
