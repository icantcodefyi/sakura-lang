# Reproduce WS-P13a

The measured compiler and workload revision is `36e3c184cea82cf3db0c2633e645e38c8ea11b3b`.
The scripts below are experiment documentation. Run them in scratch space.
Use Node 26.11.0 and Bun 1.4.3. No special engine flags were used for timing.
The original scratch directory was `/tmp/astra-p13a`.

## Build and apply the two patches

Start from the repository that contains this note. `P13_EVIDENCE` points to the
patches in this docs commit, while the new checkout pins the measured compiler.
Use fresh directory names if these paths already exist.

```sh
export P13_EVIDENCE="$PWD/research/10-evidence/ws-p13a"
export P13_WORK=/tmp/sakura-p13a-reproduce
export CARGO_TARGET_DIR=/tmp/sakura-p13a-target

git worktree add /tmp/sakura-p13a-source 36e3c184cea82cf3db0c2633e645e38c8ea11b3b --detach
cd /tmp/sakura-p13a-source
(cd compiler && cargo build --release --locked -p sakura_cli)
mkdir -p "$P13_WORK"
"$CARGO_TARGET_DIR/release/sakura" build --out-dir "$P13_WORK/base" bench/programs/deltablue/sakura/main.sak
python3 - <<'PYCODE'
import os
from pathlib import Path
base=Path(os.environ['P13_WORK'])/'base'
lines=(base/'__main__.js').read_text().splitlines()
kept=[l.replace('import { registerOps, runMain }','import { registerOps }') for l in lines if not l.startswith('runMain(') and '$entry' not in l]
kept += ['export * as program from "./main/main.js";', 'export { runTask } from "./sakura_runtime.js";']
(base/'bench_entry.mjs').write_text('\n'.join(kept)+'\n')
PYCODE
cp -R "$P13_WORK/base" "$P13_WORK/owned"
patch -d "$P13_WORK/owned" -p1 < "$P13_EVIDENCE/owned.patch"
cp -R "$P13_WORK/owned" "$P13_WORK/flat"
patch -d "$P13_WORK/flat" -p1 < "$P13_EVIDENCE/flat.patch"
```

The wrapper is the existing `bench/harness/bench.py::build_sakura` wrapper.
Runtime files remain byte-identical in all three directories. Apply `flat.patch`
after `owned.patch`, not directly to the baseline.

## Run the complete matrix

Save the following block as `$P13_WORK/measure.py`, then run the command below it
from the pinned source checkout. It checks all eight oracle inputs first and
retains all nine samples from each fresh process. A failure aborts collection.

```python
import datetime, hashlib, json, os, platform, statistics, subprocess, tomllib
from pathlib import Path
repo=Path.cwd()
root=Path(os.environ['P13_WORK'])
runner=repo/'bench/harness/runner.mjs'
variants={name:root/name/'bench_entry.mjs' for name in ['base','owned','flat']}
variants['js']=repo/'bench/programs/deltablue/js/main.mjs'
checks=tomllib.loads((repo/'bench/programs/deltablue/meta.toml').read_text())['checksums']
def command(args):
 r=subprocess.run([str(a) for a in args],capture_output=True,text=True,timeout=180)
 if r.returncode: raise RuntimeError((args,r.returncode,r.stderr))
 return json.loads(r.stdout)
data={'commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'system':platform.platform(),'cpus':os.cpu_count(),'versions':{e:subprocess.check_output([e,'--version'],text=True).strip() for e in ['node','bun']},'sha256':{name:hashlib.sha256((root/name/'main/main.js').read_bytes()).hexdigest() for name in ['base','owned','flat']},'oracles':[],'runs':[],'failures':[],'quiet':False}
try:
 for e in ['node','bun']:
  for name,path in variants.items():
   r=command([e,runner,'values',path,*checks.keys()])
   assert r['values']==checks,(e,name,r)
   data['oracles'].append({'engine':e,'variant':name,**r})
   print('oracle',e,name,'PASS',flush=True)
 for rnd in range(8):
  cells=[(e,n) for e in ['node','bun'] for n in [1000,5000]]
  if rnd%2: cells.reverse()
  names=list(variants)
  names=names[rnd%4:]+names[:rnd%4]
  for e,n in cells:
   for name in names:
    load=os.getloadavg()
    r=command([e,runner,'measure',variants[name],n])
    assert r['checksum']==checks[str(n)],(e,name,r)
    item={'round':rnd,'engine':e,'variant':name,'n':n,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'load_before':load,'load_after':os.getloadavg(),**r}
    data['runs'].append(item)
    (root/'readings.json').write_text(json.dumps(data,indent=2)+'\n')
   print('round',rnd,e,n,'done',flush=True)
except BaseException as e:
 data['failures'].append(repr(e))
 raise
finally:
 (root/'readings.json').write_text(json.dumps(data,indent=2)+'\n')

```

```sh
python3 research/10-evidence/record_load.py --log "$P13_WORK/load.jsonl" -- python3 "$P13_WORK/measure.py"
```

## Confirm the threshold cell with longer warmup

This follow-up was selected after the primary Node n=5000 result was 22.683%
shorter elapsed time with wide ranges. Its rule was fixed before collection.
It uses eight fresh processes per arm, alternates the order, and adds `--stable`
for a two-second warmup. It does not replace the primary matrix.
Save this block as `$P13_WORK/confirm.py`.

```python
import datetime,json,os,statistics,subprocess
from pathlib import Path
root=Path(os.environ['P13_WORK']); runner=Path.cwd()/'bench/harness/runner.mjs'
d={'reason':'Primary Node n=5000 reduction 22.683%, close to 25% threshold with wide ranges. Follow-up protocol fixed before collection: eight fresh processes per arm, alternating order, stable 2s warmup. Retain primary matrix.','runs':[],'quiet':False}
for rnd in range(8):
 for arm in (['base','owned'] if rnd%2==0 else ['owned','base']):
  args=['node',str(runner),'measure',str(root/arm/'bench_entry.mjs'),'5000','--stable']
  r=subprocess.run(args,capture_output=True,text=True,timeout=180)
  assert r.returncode==0,(r.returncode,r.stderr)
  out=json.loads(r.stdout);assert out['checksum']=='139986176'
  d['runs'].append({'round':rnd,'arm':arm,'load':os.getloadavg(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),**out})
  (root/'confirmation.json').write_text(json.dumps(d,indent=2)+'\n')
  print(rnd,arm,statistics.median(out['samples_ms']),flush=True)

```

```sh
python3 research/10-evidence/record_load.py --log "$P13_WORK/confirmation-load.jsonl" -- python3 "$P13_WORK/confirm.py"
```

## Profile separately

For each arm `base`, `owned`, and `flat`, run the following command with `ARM`
set to that arm. These outputs do not enter the timing table.

```sh
ARM=base
node --cpu-prof --cpu-prof-dir="$P13_WORK" --cpu-prof-name="$ARM.cpuprofile" bench/harness/runner.mjs measure "$P13_WORK/$ARM/bench_entry.mjs" 5000 --stable
```

Aggregate each process's nine `samples_ms` with `statistics.median`, then take
the median, minimum, and maximum across the eight process medians per cell.
Compute the time reduction as `100 * (1 - owned_ms / base_ms)`.
The hand-JS ratio divides each arm's median by the same cell's hand-JS median.
Do not pool all 72 samples as independent processes or discard slow processes.
The profile summary weights sampled leaf functions by `timeDeltas` and retains
all ticks, including GC and startup. It is a separate diagnostic.

## Regenerate the JS profile summary

Run this block after the three profiler commands. For retained evidence,
decompress each `*.cpuprofile.gz` into `$P13_WORK` first.

```python
import collections, json, os
from pathlib import Path
root = Path(os.environ['P13_WORK'])
summary = {}
for arm in ['base', 'owned', 'flat']:
    profile = json.loads((root / (arm + '.cpuprofile')).read_text())
    nodes = {node['id']: node for node in profile['nodes']}
    counts = collections.Counter()
    for node_id, duration in zip(profile['samples'], profile['timeDeltas']):
        frame = nodes[node_id]['callFrame']
        counts[(frame['functionName'], Path(frame['url']).name, frame['lineNumber'])] += duration
    total = sum(counts.values())
    summary[arm] = {
        'total_sampled_us': total,
        'samples': len(profile['samples']),
        'functions': [
            {'name': key[0], 'file': key[1], 'line_zero_based': key[2],
             'sampled_us': value, 'percent': 100 * value / total}
            for key, value in counts.most_common()
        ],
    }
(root / 'js-profile-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
```

The confirmation script's `139986176` checksum is the frozen revision's
`meta.toml` value for n=5000, not a newly generated oracle.
