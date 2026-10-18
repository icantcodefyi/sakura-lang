# Diagnostic research only. Run from the repository root on macOS / Node 26.
import json,os,subprocess,time,random
from pathlib import Path
p=Path(__file__).resolve().parents[1]/"target/btrss"
root=p.parents[3]
mods={'direct':p/'direct/bench_entry.mjs','cps':p/'cps/bench_entry.mjs','js':root/'bench/programs/binary_trees/js/main.mjs'}
configs={'default':[], 'semi1':['--max-semi-space-size=1'], 'semi4':['--max-semi-space-size=4'], 'semi16':['--max-semi-space-size=16'], 'semi64':['--max-semi-space-size=64'], 'old64':['--max-old-space-size=64'], 'old128':['--max-old-space-size=128'], 'no-pretenure':['--no-allocation-site-pretenuring'], 'growth30':['--heap-growing-percent=30']}
jobs=[(c,v,rep) for c in configs for v in mods for rep in range(3)];random.Random(20261018).shuffle(jobs)
results=[]
for c,v,rep in jobs:
 path=p/f'matrix-{c}-{v}-{rep}.log'
 cmd=['node','--expose-gc',*configs[c],str(root/'research/10-evidence/binary-trees-rss.mjs'),str(mods[v]),'20']
 with path.open('w') as f:
  proc=subprocess.Popen(cmd,stdout=f,stderr=subprocess.STDOUT)
  _,status,usage=os.wait4(proc.pid,0);proc.returncode=os.waitstatus_to_exitcode(status)
 if proc.returncode: raise RuntimeError((cmd,path.read_text()))
 d=json.loads(path.read_text().splitlines()[-1]);results.append({'config':c,'variant':v,'rep':rep,'rss_bytes':usage.ru_maxrss,'elapsed_ms':d['elapsedMs'],'end_peak_heap_used':max(s['heapUsed'] for s in d['samples'][:-1]),'post_gc_heap_used':d['samples'][-1]['heapUsed']})
 (p/'matrix.json').write_text(json.dumps(results,indent=2)+'\n')
 print(c,v,rep,round(usage.ru_maxrss/1e6,1),flush=True)
