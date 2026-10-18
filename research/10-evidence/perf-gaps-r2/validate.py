"""Offline integrity check for the committed research evidence, not a perf gate."""
from pathlib import Path
import collections,gzip,hashlib,json,re,statistics,ast
E=Path(__file__).resolve().parent
load=lambda name:json.loads((E/name).read_text())
m=load('measurements.json');manifest=load('manifest.json');summary=load('summary.json')
assert m['commit']==manifest['commit']==load('provenance.json')['commit']
assert len(m['rows'])==310 and len(m['oracles'])==62
expected={(p,e,v) for p,vs in manifest['variants'].items() for e in ['node','bun'] for v in vs+['hand']}
g=collections.defaultdict(list)
oracles={(r['program'],r['engine'],r['variant']):r['values'] for r in m['oracles']}
assert set(oracles)==expected
for r in m['rows']:
 k=(r['program'],r['engine'],r['variant']);g[k].append(r)
 assert len(r['samples_ms'])==9 and all(x>0 for x in r['samples_ms'])
 assert r['median_ms']==statistics.median(r['samples_ms'])
 assert r['checksum']==oracles[k][str(r['n'])]
assert set(g)==expected and len(summary)==62
for s in summary:
 rows=g[s['program'],s['engine'],s['variant']];v=[r['median_ms'] for r in rows]
 assert sorted(r['repetition'] for r in rows)==list(range(5))
 assert s['ms']==statistics.median(v) and s['min_ms']==min(v) and s['max_ms']==max(v)
 base=statistics.median(r['median_ms'] for r in g[s['program'],s['engine'],'base'])
 assert s['speedup']==base/s['ms']
f=load('followup.json'); assert len(f['rows'])==30 and len(f['oracles'])==6
for r in f['rows']:
 assert len(r['samples_ms'])==9 and r['median_ms']==statistics.median(r['samples_ms'])
 assert r['checksum']==oracles['spectral_norm',r['engine'],'base']['400']
for name,profile in load('profiles.json').items():
 if 'self' not in profile:continue
 raw=gzip.decompress((E/(name.replace('/','-')+'.cpuprofile.gz')).read_bytes())
 assert hashlib.sha256(raw).hexdigest()==profile['sha256']
 assert sum(r['samples'] for r in profile['self'])==profile['samples']
for p,r in load('counts.json').items():
 assert str(r['result']['value'])==oracles[p,'node','base'][str({'sieve':200000,'nbody':100000,'spectral_norm':400}[p])]
 assert r['counts']['ret']==1 and r['counts'].get('bounce',0)==r['counts'].get('runSync',0)==0
for path in [E/'README.md',E.parent/'perf-gaps-r2.md']:
 for link in re.findall(r'\]\(([^)]+)\)',path.read_text()):
  if '://' not in link and not link.startswith('#'):
   assert (path.parent/link.split('#')[0]).exists(),(path,link)
for p in E.glob('*.py'):ast.parse(p.read_text(),filename=str(p))
print('PASS: 340 timed processes, 3,060 batches, 68 oracle runs; summaries, profile hashes, counts, local links and Python syntax')
