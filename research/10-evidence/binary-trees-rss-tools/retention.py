# Additional last-use/root experiment; all mutations stay in ignored generated output.
import json, os, subprocess, shutil
from pathlib import Path
p=Path(__file__).resolve().parents[1]/'target/btrss';root=p.parents[3]
rows=[]
def run(name,args):
 with (p/(name+'.log')).open('w') as out:
  proc=subprocess.Popen(['node',*args],stdout=out,stderr=subprocess.STDOUT)
  _,status,u=os.wait4(proc.pid,0);proc.returncode=os.waitstatus_to_exitcode(status)
 if proc.returncode:raise RuntimeError(name)
 rows.append({'name':name,'rss_bytes':u.ru_maxrss,'result':json.loads((p/(name+'.log')).read_text().splitlines()[-1])})
 (p/'retention.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(name,round(u.ru_maxrss/1e6,1),flush=True)
# Both preserve the exact Tree/fields representation and runtime.
for variant in ['direct-inline','direct-let-only']:
 d=p/variant;shutil.copytree(p/'direct',d,dirs_exist_ok=True);f=d/'main/main.js';s=f.read_text()
 if variant=='direct-inline':s=s.replace('const r_1 = bottom_up$d(t_1);\n  const r_2 = check$d(r_1);','const r_2 = check$d(bottom_up$d(t_1));')
 else:s=s.replace('const r_1 = bottom_up$d(t_1);','let r_1 = bottom_up$d(t_1);')
 f.write_text(s)
 for rep in range(3):run(f'{variant}-{rep}',['--expose-gc',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(d/'bench_entry.mjs'),'20'])
for v in ['direct-inline','direct-clearstretch','direct-newarray']:
 for rep in range(5):run(f'harness-{v}-{rep}',[str(root/'bench/harness/runner.mjs'),'measure',str(p/v/'bench_entry.mjs'),'14'])
for v in ['direct','cps']:
 run(v+'-heappeak',['--expose-gc',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(p/(v+'-probe')/'bench_entry.mjs'),'20','-95000000',str(p/(v+'-heappeak'))])
for v in ['direct','cps','direct-clearstretch','direct-inline']:
 base=p/(v+'-probe') if v in ['direct','cps'] else p/v
 d=p/(v+'-first-tree');shutil.copytree(base,d,dirs_exist_ok=True);f=d/'main/main.js';s=f.read_text()
 if v in ['direct','cps']:s=s.replace('if (!(i_1 % 256) || d_4 >= 12)', 'if (d_4 == 4 && i_1 == 0)')
 else:s=s.replace('const r_5 = bottom_up$d(d_4);','const r_5 = bottom_up$d(d_4); if (d_4 == 4 && i_1 == 0) globalThis.__btSample("tree", d_4, i_1);')
 f.write_text(s)
 run(v+'-first',['--expose-gc',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(d/'bench_entry.mjs'),'1','-12000000',str(p/(v+'-first'))])
for v in ['direct-inline','direct-clearstretch','direct-newarray']:
 run(v+'-gc',['--expose-gc','--trace-gc','--trace-gc-verbose','--trace-gc-nvp','--trace-pretenuring','--trace-pretenuring-statistics',str(root/'research/10-evidence/binary-trees-rss.mjs'),str(p/v/'bench_entry.mjs'),'20'])
