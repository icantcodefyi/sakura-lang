"""Interleaved fresh-process ablations; unmodified bench runner, all oracle sizes."""
from pathlib import Path
import subprocess, json, random, statistics, time, tomllib, os, platform
ROOT=Path.cwd(); E=ROOT/'research/10-evidence/perf-gaps-r2'
OUT=Path(os.environ.get('PERF_TMP','/tmp/sakura-perfgaps-r2'))
runner=ROOT/'bench/harness/runner.mjs'
manifest=json.loads((E/'manifest.json').read_text())
sizes={'sieve':200000,'nbody':100000,'spectral_norm':400}
data={'commit':manifest['commit'],'host':platform.platform(),'engines':{e:subprocess.check_output([e,'--version'],text=True).strip() for e in ['node','bun']},'seed':20261018,'processes':5,'warmup_ms':250,'samples_per_process':9,'rows':[],'oracles':[],'load':[]}
def module(p,v): return ROOT/f'bench/programs/{p}/js/main.mjs' if v=='hand' else OUT/p/v/'bench_entry.mjs'
def call(e,p,v,mode,*args):
 cmd=[e,str(runner),mode,str(module(p,v)),*map(str,args)]
 r=subprocess.run(cmd,capture_output=True,text=True,timeout=180)
 if r.returncode: raise RuntimeError((cmd,r.stderr))
 return json.loads(r.stdout.strip().splitlines()[-1])
def save(): (E/'measurements.json').write_text(json.dumps(data,indent=2)+'\n')
for p,vs in manifest['variants'].items():
 want=tomllib.loads((ROOT/f'bench/programs/{p}/meta.toml').read_text())['checksums']
 for e in ['node','bun']:
  for v in vs+['hand']:
   got=call(e,p,v,'values',*want)['values']; assert got==want,(p,e,v,got,want)
   data['oracles'].append({'program':p,'engine':e,'variant':v,'values':got})
 print('oracles passed',p,flush=True)
save()
rng=random.Random(data['seed'])
for rep in range(5):
 jobs=[(p,e,v) for p,vs in manifest['variants'].items() for e in ['node','bun'] for v in vs+['hand']]
 rng.shuffle(jobs)
 for p,e,v in jobs:
  load={'time':time.time(),'uptime':subprocess.check_output(['uptime'],text=True).strip()}
  data['load'].append(load)
  r=call(e,p,v,'measure',sizes[p])
  want=tomllib.loads((ROOT/f'bench/programs/{p}/meta.toml').read_text())['checksums'][str(sizes[p])]
  assert r['checksum']==want,(p,e,v,r)
  r.update(program=p,engine=e,variant=v,repetition=rep,median_ms=statistics.median(r['samples_ms']),time=time.time())
  data['rows'].append(r); save()
 print('completed repetition',rep+1,flush=True)
print('all done',len(data['rows']),flush=True)
