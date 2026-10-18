# Diagnostic research only. Run from the repository root on macOS / Node 26.
from pathlib import Path
import shutil, subprocess, os
p=Path(__file__).resolve().parents[1]/"target/btrss"
root=p.parents[3]
os.chdir(root)
p.mkdir(parents=True,exist_ok=True)
cli=Path(os.environ['CARGO_TARGET_DIR'])/'release/sakura'
subprocess.run(['cargo','build','--locked','--release','-p','sakura_cli'],cwd=root/'compiler',check=True)
for variant,flags in [('direct',[]),('cps',['--no-direct'])]:
 subprocess.run([str(cli),'build',*flags,'--out-dir',str(p/variant),'bench/programs/binary_trees/sakura/main.sakura'],check=True)
 d=p/variant;lines=(d/'__main__.js').read_text().splitlines()
 kept=[l.replace('import { registerOps, runMain }','import { registerOps }') for l in lines if not l.startswith('runMain(') and '$entry' not in l]
 (d/'bench_entry.mjs').write_text('\n'.join(kept+['export * as program from "./main/main.js";', 'export { runTask } from "./sakura_runtime.js";'])+'\n')
with (p/'direct.txt').open('w') as f:
 subprocess.run([str(cli),'build','--dump','direct','bench/programs/binary_trees/sakura/main.sakura'],stdout=f,check=True)
for v in ['direct','cps']:
 d=p/(v+'-probe');shutil.copytree(p/v,d,dirs_exist_ok=True)
 f=d/'main/main.js';s=f.read_text()
 s=s.replace('const r_5 = bottom_up$d(d_4);','const r_5 = bottom_up$d(d_4);\n                  if (!(i_1 % 256) || d_4 >= 12) globalThis.__btSample("tree", d_4, i_1);')
 s=s.replace('return bottom_up(d_4, $ctx, (r_5) => {','return bottom_up(d_4, $ctx, (r_5) => {\n                      if (!(i_1 % 256) || d_4 >= 12) globalThis.__btSample("tree", d_4, i_1);')
 f.write_text(s)
 f=d/'sakura_runtime.js';s=f.read_text().replace('export function runSync(f, args) {','export function runSync(f, args) {\n  globalThis.__btCounters.sync++;')
 s=s.replace('constructor(f, a) {','constructor(f, a) {\n    globalThis.__btCounters.bounce++;')
 f.write_text(s)
# Gauge counters separately: a hot-path max() would perturb timing on every recursive call.
d=p/'direct-gauge';shutil.copytree(p/'direct-probe',d,dirs_exist_ok=True)
f=d/'main/main.js';s=f.read_text().replace('++$G.d;', '++$G.d; globalThis.__btCounters.maxGauge = Math.max(globalThis.__btCounters.maxGauge, $G.d);');f.write_text(s)
s=Path('bench/programs/binary_trees/js/main.mjs').read_text()
s=s.replace('for (let i = 0; i < iterations; i++) total += check(bottomUp(d));','for (let i = 0; i < iterations; i++) { const tree = bottomUp(d); if (!(i % 256) || d >= 12) globalThis.__btSample("tree", d, i); total += check(tree); }')
(p/'js-probe.mjs').write_text(s)
