import fs from 'node:fs';
import {pathToFileURL} from 'node:url';
const [wasm, mode, count = '25', runtime] = process.argv.slice(2);
const {instance} = await WebAssembly.instantiate(fs.readFileSync(wasm));
const e = instance.exports;
if (mode === 'division') {
  const rt = await import(pathToFileURL(runtime));
  const cases = [];
  for (const a of [-17, -7, -1, 0, 1, 7, 17]) {
    for (const b of [-5, -3, -1, 1, 3, 5]) {
      const q = Number(e.div(BigInt(a), BigInt(b)));
      const r = Number(e.rem(BigInt(a), BigInt(b)));
      if (q !== rt.div(a,b) || r !== rt.rem(a,b)) throw Error('division mismatch');
      cases.push({a,b,q,r});
    }
  }
  console.log(JSON.stringify({profile:e.profile(), cases}));
} else if (mode === 'profile') {
  console.log(JSON.stringify(e.profile()));
} else {
  console.log(e[mode](Number(count)));
}
