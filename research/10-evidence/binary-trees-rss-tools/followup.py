# Diagnostic research only. Run from the repository root on macOS / Node 26.
import json,os,subprocess,shutil,re
from pathlib import Path
p=Path(__file__).resolve().parents[1]/"target/btrss";root=p.parents[3]
results=[]
def run(name,cmd,env=None):
 out=p/(name+'.log')
 with out.open('w') as f:
  proc=subprocess.Popen(cmd,stdout=f,stderr=subprocess.STDOUT,env=env)
  _,status,u=os.wait4(proc.pid,0);proc.returncode=os.waitstatus_to_exitcode(status)
 if proc.returncode:raise RuntimeError((cmd,out.read_text()[-2000:]))
 row={'name':name,'cmd':cmd,'rss_bytes':u.ru_maxrss,'result':json.loads(out.read_text().splitlines()[-1])}
 results.append(row);(p/'followup.json').write_text(json.dumps(results,indent=2)+'\n');print(name,round(u.ru_maxrss/1e6,1),flush=True)
mods={v:p/v/'bench_entry.mjs' for v in ['direct','cps']};mods['js']=root/'bench/programs/binary_trees/js/main.mjs'
for v in mods:
 for rep in range(5):run(f'harness-{v}-{rep}',['node',str(root/'bench/harness/runner.mjs'),'measure',str(mods[v]),'14'])
for v in mods:
 run(f'equal-gc-{v}',['node','--expose-gc','--trace-gc','--trace-gc-verbose','--trace-gc-nvp','--trace-pretenuring','--trace-pretenuring-statistics',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(mods[v]),'20'])
for v,threshold in [('direct',200000000),('cps',160000000),('js',110000000)]:
 mod=p/(v+'-probe')/'bench_entry.mjs' if v!='js' else p/'js-probe.mjs'
 run(f'snapshot-{v}',['node','--expose-gc',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(mod),'20',str(threshold),str(p/v)])
 run(f'profile-{v}',['node','--expose-gc',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(mods[v]),'20','0',str(p/v)],os.environ|{'BT_PROFILE':'1'})
# Diagnostic-only generated JS interventions; no source compiler/runtime edits.
for variant in ['direct-flat','direct-newarray','direct-clearstretch']:
 d=p/variant;shutil.copytree(p/'direct',d,dirs_exist_ok=True);f=d/'main/main.js';s=f.read_text()
 if variant=='direct-flat':
  s=s.replace('export class Tree extends $rt.Variant {}','export class Tree { constructor(tag, l, r) { this.tag = tag; this.l = l; this.r = r; } }')
  s=s.replace('new Tree(0, [])','new Tree(0)').replace('new Tree(1, [r, r_1])','new Tree(1, r, r_1)').replace('new Tree(1, [v_1, v_2])','new Tree(1, v_1, v_2)').replace('t.fields[0]','t.l').replace('t.fields[1]','t.r')
 elif variant=='direct-newarray':
  s=s.replace('[r, r_1]', 'new Array(r, r_1)').replace('[v_1, v_2]', 'new Array(v_1, v_2)')
 else:
  s=s.replace('const r_1 = bottom_up$d(t_1);\n  const r_2 = check$d(r_1);','let r_1 = bottom_up$d(t_1);\n  const r_2 = check$d(r_1);\n  r_1 = null;')
 f.write_text(s)
 for rep in range(3):run(f'{variant}-{rep}',['node','--expose-gc',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(d/'bench_entry.mjs'),'20'])

run("gauge",["node","--expose-gc",str(root/"research/10-evidence/binary-trees-rss.mjs"),str(p/"direct-gauge/bench_entry.mjs"),"20"])
(p/"gauge.json").write_text(json.dumps(results[-1]["result"])+"\n")
