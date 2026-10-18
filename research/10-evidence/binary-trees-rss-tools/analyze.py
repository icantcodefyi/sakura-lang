# Diagnostic research only. Run from the repository root on macOS / Node 26.
import json,re,collections,statistics
from pathlib import Path
p=Path(__file__).resolve().parents[1]/"target/btrss"
summary={}
for v in ['direct','cps','js','direct-inline','direct-clearstretch','direct-newarray']:
 f=p/(f'equal-gc-{v}.log' if v in ['direct','cps','js'] else f'{v}-gc.log')
 if not f.exists():continue
 s=f.read_text();rows=[json.loads(l.split('GC: ',1)[1]) for l in s.splitlines() if 'GC: {' in l]
 def mx(pattern):
  vals=re.findall(pattern,s);return max(map(float,vals),default=0)
 summary[v]={'gc_count':dict(collections.Counter(r['gc'] for r in rows)), 'allocated_bytes':sum(r['allocated'] for r in rows), 'promoted_bytes':sum(r['promoted'] for r in rows), 'peak_before_gc_bytes':max(r['start_object_size'] for r in rows),'max_old_used_kb':mx(r'Old space,\s+used:\s+(\d+)'), 'max_old_committed_kb':mx(r'Old space,.*committed:\s+(\d+)'), 'max_new_committed_kb':mx(r'New space,.*committed:\s+(\d+)'), 'max_pool_committed_kb':mx(r'Pool buffering.*committed:\s+(\d+)'), 'pretenuring':[l.split('] ',1)[1] for l in s.splitlines() if 'AllocationSite' in l], 'major_gcs':[{k:r.get(k) for k in ['gc','start_object_size','end_object_size','start_memory_size','end_memory_size','old_gen_allocation_limit','allocated','promoted']} for r in rows if r['gc']!='s'], 'result':json.loads(s.splitlines()[-1])}
 # Retain machine-readable GC events, not bulky verbose logging.
 keys=['gc','pause','mutator','start_object_size','end_object_size','start_memory_size','end_memory_size','new_space_capacity','old_gen_allocation_limit','allocated','promoted','new_space_survived','promotion_ratio','promotion_rate','new_space_survive_rate','allocation_throughput','new_space_allocation_throughput']
 (p/f'{v}-gc-events.json').write_text(json.dumps([{k:r.get(k) for k in keys} for r in rows],separators=(',',':'))+'\n')
print('GC',json.dumps({v:{k:d for k,d in s.items() if k not in ['result','pretenuring']} for v,s in summary.items()},indent=2))
for v in ['direct','cps','js','direct-heappeak','cps-heappeak','direct-first','cps-first','direct-inline-first','direct-clearstretch-first']:
 f=p/f'{v}.heapsnapshot'
 if not f.exists():continue
 d=json.loads(f.read_text());meta=d['snapshot']['meta'];nf=meta['node_fields'];ef=meta['edge_fields'];ns=len(nf);es=len(ef);nodes=d['nodes'];edges=d['edges'];strings=d['strings'];types=meta['node_types'][0];etypes=meta['edge_types'][0]
 offsets=[];o=0
 for i in range(0,len(nodes),ns):offsets.append(o);o+=nodes[i+nf.index('edge_count')]*es
 def info(i):return types[nodes[i+nf.index('type')]],strings[nodes[i+nf.index('name')]],nodes[i+nf.index('self_size')]
 def links(i):
  begin=offsets[i//ns];end=begin+nodes[i+nf.index('edge_count')]*es
  for j in range(begin,end,es):
   typ=etypes[edges[j]];name=edges[j+1] if typ in ['element','hidden'] else strings[edges[j+1]]
   yield typ,name,edges[j+2]
 counts=collections.Counter();sizes=collections.Counter();owned=set();trees=[];tree_roots=[];plain_trees=[]
 for i in range(0,len(nodes),ns):
  typ,name,size=info(i);counts[(typ,name)]+=1;sizes[(typ,name)]+=size
  if typ=='object' and name=='Object' and {'l','r'}.issubset({key for typ,key,_ in links(i) if typ=='property'}):plain_trees.append(i)
  if typ=='object' and name=='Tree':
   trees.append(i);owned.add(i)
   for _,key,to in links(i):
    if key=='fields':
     owned.add(to)
     for _,key2,to2 in links(to):
      if key2=='elements':owned.add(to2)
 # Root Trees have a non-elements owner; normal child links come from backing arrays.
 tree_set=set(trees)
 for i in range(0,len(nodes),ns):
  for typ,key,to in links(i):
   if to in tree_set and info(i)[0]!='array' and not (info(i)[1]=='Array' and typ=='element'):
    tree_roots.append({'from':info(i)[:2],'edge':[typ,key],'to_id':nodes[to+nf.index('id')]})
 heap={'nodes':len(nodes)//ns,'total_self_bytes':sum(nodes[i+nf.index('self_size')] for i in range(0,len(nodes),ns)), 'plainTree_count':len(plain_trees),'plainTree_self_bytes':sum(info(i)[2] for i in plain_trees),'Tree_count':len(trees),'Tree_and_fields_bytes':sum(info(i)[2] for i in owned),'Tree_self_sizes':dict(collections.Counter(info(i)[2] for i in trees)), 'top_by_bytes':[{'type':k[0],'name':('[string omitted]' if k[0]=='string' else k[1]),'count':counts[k],'bytes':n} for k,n in sizes.most_common(15)],'closure_count':sum(n for (t,_),n in counts.items() if t=='closure'),'closure_bytes':sum(n for (t,_),n in sizes.items() if t=='closure'), 'contexts':{name:{'count':counts[(typ,name)],'bytes':n} for (typ,name),n in sizes.items() if name=='system / Context'}, 'Bounce_count':counts[('object','Bounce')],'Done_count':counts[('object','Done')], 'Tree_roots':tree_roots[:30]}
 summary.setdefault(v,{})['snapshot']=heap
 print('SNAPSHOT',v,json.dumps(heap,indent=2))
for v in ['direct','cps','js']:
 f=p/f'{v}.heapprofile'
 if not f.exists():continue
 d=json.loads(f.read_text());by=collections.Counter();byfunc=collections.Counter()
 def visit(n):
  c=n['callFrame'];key=f"{c['functionName']} {c['url'].split('/')[-1]}:{c['lineNumber']+1}"
  by[key]+=n['selfSize'];byfunc[c['functionName']]+=n['selfSize']
  for x in n['children']:visit(x)
 visit(d['head']);prof={'total_sampled_bytes':sum(by.values()),'top_allocations':by.most_common(12)};summary.setdefault(v,{})['profile']=prof;print('PROFILE',v,prof)
(p/'analysis.json').write_text(json.dumps(summary,indent=2)+'\n')
