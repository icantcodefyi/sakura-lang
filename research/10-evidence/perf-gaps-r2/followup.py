"""Preselected after pilot: isolate Option elimination from leaf caching.
Run only after measure.py completes. Fresh baseline/fused controls are interleaved.
"""
from pathlib import Path
import shutil,subprocess,json,random,statistics,tomllib,difflib,time
from prepare import OUT,EVIDENCE as E,ROOT,body,RT
p='spectral_norm';d=OUT/p/'fused_uncached'
shutil.copytree(OUT/p/'fused_at',d,dirs_exist_ok=True)
rt=(d/RT).read_text(); changed=body(rt,'list_at','  return at(xs, i);'); (d/RT).write_text(changed)
(E/'followup.patch').write_text(''.join(difflib.unified_diff(rt.splitlines(True),changed.splitlines(True),fromfile='spectral_norm/fused_at/'+RT,tofile='spectral_norm/fused_uncached/'+RT,n=0)))
runner=ROOT/'bench/harness/runner.mjs'
want=tomllib.loads((ROOT/f'bench/programs/{p}/meta.toml').read_text())['checksums']
variants=['base','fused_at','fused_uncached']; rows=[];oracles=[]
for e in ['node','bun']:
 for v in variants:
  got=json.loads(subprocess.check_output([e,str(runner),'values',str(OUT/p/v/'bench_entry.mjs'),*want],text=True))['values']
  assert got==want
  oracles.append({'engine':e,'variant':v,'values':got})
rng=random.Random(20261019)
for rep in range(5):
 jobs=[(e,v) for e in ['node','bun'] for v in variants];rng.shuffle(jobs)
 for e,v in jobs:
  uptime=subprocess.check_output(['uptime'],text=True).strip()
  r=json.loads(subprocess.check_output([e,str(runner),'measure',str(OUT/p/v/'bench_entry.mjs'),'400'],text=True))
  assert r['checksum']==want['400']
  rows.append(r|{'engine':e,'variant':v,'repetition':rep,'median_ms':statistics.median(r['samples_ms']),'uptime':uptime,'time':time.time()})
 (E/'followup.json').write_text(json.dumps({'seed':20261019,'rows':rows,'oracles':oracles},indent=2)+'\n')
print('followup complete',len(rows))
