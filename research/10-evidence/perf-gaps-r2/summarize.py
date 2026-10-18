from pathlib import Path
import json,statistics,collections,re
E=Path(__file__).resolve().parent
d=json.loads((E/'measurements.json').read_text())
groups=collections.defaultdict(list)
for r in d['rows']: groups[r['program'],r['engine'],r['variant']].append(r)
summary=[]; md=[]
for p in ['sieve','nbody','spectral_norm']:
 md += [f'## {p}', '', '| Variant | Node ms median [min–max] | Node speedup | Bun ms median [min–max] | Bun speedup |','|---|---:|---:|---:|---:|']
 variants=sorted({v for q,e,v in groups if p==q})
 for v in variants:
  cells=[]
  for e in ['node','bun']:
   rows=groups[p,e,v]; vals=[r['median_ms'] for r in rows]; med=statistics.median(vals)
   base=groups[p,e,'base']; bmed=statistics.median(r['median_ms'] for r in base)
   paired=[next(r['median_ms'] for r in base if r['repetition']==x['repetition'])/x['median_ms'] for x in rows]
   item={'program':p,'engine':e,'variant':v,'n':rows[0]['n'],'processes':len(vals),'ms':med,'min_ms':min(vals),'max_ms':max(vals),'spread_over_median_pct':100*(max(vals)-min(vals))/med,'speedup':bmed/med,'paired_speedup_min':min(paired),'paired_speedup_max':max(paired)}
   summary.append(item);cells += [f'{med:.3f} [{min(vals):.3f}–{max(vals):.3f}]',f'{bmed/med:.2f}×']
  md.append('| '+v+' | '+' | '.join(cells)+' |')
 md.append('')
(E/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
(E/'tables.md').write_text('\n'.join(md).rstrip()+'\n')
loads=[]
for r in d['load']:
 m=re.search(r'load averages?:\s*([\d.]+)',r['uptime'])
 if m: loads.append(float(m[1]))
print('rows',len(d['rows']),'oracles',len(d['oracles']),'load min/median/max',min(loads),statistics.median(loads),max(loads))
print('\n'.join(md))
