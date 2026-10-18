"""Run separately, after measure.py completes, to avoid self-interference."""
from pathlib import Path
import subprocess, json, shutil, collections, gzip, hashlib
from prepare import OUT, EVIDENCE as E, ROOT, body, MAIN, RT
sizes={'sieve':200000,'nbody':100000,'spectral_norm':400}
profiles=OUT/'profiles';profiles.mkdir(exist_ok=True)
summary={};counts={}
for p,n in sizes.items():
 entry=OUT/p/'base/bench_entry.mjs'
 for engine in ['node','bun']:
  name=f'{p}-{engine}.cpuprofile'
  cmd=[engine,'--cpu-prof',f'--cpu-prof-dir={profiles}',f'--cpu-prof-name={name}',str(E/'profile.mjs'),str(entry),str(n),'3']
  r=subprocess.run(cmd,check=True,capture_output=True,text=True)
  raw=(profiles/name).read_bytes(); prof=json.loads(raw)
  hits=collections.Counter(prof.get('samples',[]))
  nodes={x['id']:x for x in prof['nodes']}
  rows=[]
  for id,hit in hits.items():
   f=nodes[id]['callFrame']; rows.append({'function':f['functionName'],'file':f.get('url',''),'line':f.get('lineNumber',-1)+1,'samples':hit,'percent':100*hit/sum(hits.values())})
  rows.sort(key=lambda x:-x['samples'])
  summary[f'{p}/{engine}']={'command':cmd,'stdout':r.stdout,'stderr':r.stderr,'samples':sum(hits.values()),'self':rows,'sha256':hashlib.sha256(raw).hexdigest()}
  # Compact retained raw profiles; preserve full call trees, not just top functions.
  with gzip.open(E/(name+'.gz'),'wb') as f:f.write(raw)
 log=profiles/f'{p}-v8.log'
 cmd=['node','--prof','--no-logfile-per-isolate',f'--logfile={log}',str(E/'profile.mjs'),str(entry),str(n),'3']
 r=subprocess.run(cmd,check=True,capture_output=True,text=True)
 processed=subprocess.run(['node','--prof-process',str(log)],check=True,capture_output=True,text=True)
 (E/f'{p}-v8-prof.txt').write_text(processed.stdout.rstrip()+'\n')
 summary[f'{p}/v8']={'command':cmd,'stdout':r.stdout,'stderr':r.stderr,'process_stderr':processed.stderr,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()}
 # Instrument generated copy only. Counts include one complete run after module init.
 d=OUT/p/'counted';shutil.copytree(OUT/p/'base',d,dirs_exist_ok=True)
 rt=(d/RT).read_text()
 rt='export const COST = Object.create(null);\nconst tick = (k,n=1) => COST[k]=(COST[k]||0)+n;\n'+rt
 for name in ['list_set','list_slice','list_push','list_get','list_at','Some','contract_enter','contract_exit','ret','bounce','runSync','float_sqrt']:
  import re
  rt=re.sub(r'((?:export )?function '+re.escape(name)+r'\([^\n]*\) \{)',lambda m:m[0]+f'\n  tick("{name}");',rt,count=1)
 rt=rt.replace('constructor(n, shift, root, tail, tl) {','constructor(n, shift, root, tail, tl) {\n    tick("List");')
 # Count only path-copy arrays inside setAt, not unrelated slices.
 a=rt.index('function setAt(');b=rt.index('\nexport function list_empty',a)
 sub=rt[a:b].replace('const t = xs.tail.slice(0, xs.tl);','tick("set_arrays"); tick("set_slots",xs.tl);\n    const t = xs.tail.slice(0, xs.tl);').replace('const root = xs.root.slice();','tick("set_arrays"); tick("set_slots",xs.root.length);\n  const root = xs.root.slice();').replace('copy = copy[j] = copy[j].slice();','tick("set_arrays"); tick("set_slots",copy[j].length);\n    copy = copy[j] = copy[j].slice();')
 rt=rt[:a]+sub+rt[b:];(d/RT).write_text(rt)
 main=(d/MAIN).read_text().replace('constructor($0, $1, $2, $3, $4, $5, $6) {','constructor($0, $1, $2, $3, $4, $5, $6) {\n    $rt.COST.Body = ($rt.COST.Body || 0) + 1;')
 (d/MAIN).write_text(main)
 code=f'''import * as mod from {json.dumps(str(d/'bench_entry.mjs'))};
import {{COST}} from {json.dumps(str(d/RT))};
for (const k of Object.keys(COST)) delete COST[k];
const result=mod.runTask(mod.program.run,[{n}]);
console.log(JSON.stringify({{result,counts:COST}}));'''
 r=subprocess.run(['node','--input-type=module','-e',code],check=True,capture_output=True,text=True)
 counts[p]=json.loads(r.stdout)
 print(p,'profiled and counted',flush=True)
(E/'profiles.json').write_text(json.dumps(summary,indent=2)+'\n')
(E/'counts.json').write_text(json.dumps(counts,indent=2)+'\n')
