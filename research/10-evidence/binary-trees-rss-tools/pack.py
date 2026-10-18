# Preserve compact observations and complete GC traces; large profiles/snapshots stay in target/.
import gzip, hashlib, json, os, platform, subprocess
from pathlib import Path
p=Path(__file__).resolve().parents[1]/'target/btrss';root=p.parents[3]
def compact(d):
 if 'samples' not in d:return d
 samples=d['samples'];out={k:v for k,v in d.items() if k!='samples'}
 out['sample_count']=len(samples)
 out['max_sampled']={k:max(s[k] for s in samples if s['phase']!='forced-gc') for k in ['rss','heapUsed','heapTotal','external']}
 out['post_gc']=samples[-1]
 out['snapshot_samples']=[samples[j] for i,s in enumerate(samples) if s['phase']=='after-snapshot' for j in [i-1,i]]
 return out
analysis=json.loads((p/'analysis.json').read_text())
for v in analysis.values():
 if 'result' in v:v['result']=compact(v['result'])
runs={}
for key in ['followup','retention']:
 runs[key]=json.loads((p/(key+'.json')).read_text())
 for r in runs[key]:r['result']=compact(r['result'])
 for r in runs[key]:
  result=r['result'];assert str(result['checksum'])=='3222190',r['name']
mods=[p/'direct/main/main.js',p/'cps/main/main.js',p/'direct/sakura_runtime.js',root/'bench/programs/binary_trees/js/main.mjs',root/'bench/programs/binary_trees/sakura/main.sak']
results={'provenance':{'measured_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'node':json.loads(subprocess.check_output(['node','-p','JSON.stringify({node:process.version,v8:process.versions.v8,arch:process.arch,platform:process.platform})'],text=True)), 'platform':platform.platform(),'cargo_target_dir':os.environ['CARGO_TARGET_DIR'],'sha256':{str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest() for f in mods},'environment':{k:os.environ.get(k) for k in ['NODE_OPTIONS','SAKURA_TRACE','BUN_OPTIONS']},'scope':'n=14, shared host; source compiler/runtime unchanged from WS-P3; mutations only in ignored generated copies; MB decimal; heap flags MiB; snapshot/profiler RSS is not benchmark RSS'}, 'matrix':json.loads((p/'matrix.json').read_text()),'runs':runs,'analysis':analysis,'gauge':compact(json.loads((p/'gauge.json').read_text()))}
(p/'results.json').write_text(json.dumps(results,indent=2)+'\n')
traces={f.name:f.read_text() for pattern in ['equal-gc-*.log','direct-*-gc.log'] for f in p.glob(pattern)}
with gzip.GzipFile(filename=str(p/'traces.json.gz'),mode='wb',mtime=0) as f:f.write(json.dumps(traces).encode())
print('Packed',len(results['matrix']),'matrix runs;',len(traces),'traces')
