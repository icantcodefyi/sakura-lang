# Reproduce WS-W3

Use compiler revision `36e3c184cea82cf3db0c2633e645e38c8ea11b3b` and Wasmtime 49.0.3.
The compiler build and checkout command are in [WS-P13a reproduction](../ws-p13a/reproduce.md).
Run all commands from that source checkout. The original scratch path was
`/tmp/astra-p13a`. No runtime or compiler source is edited.

## Build the measured modules

```sh
export W3_WORK=/tmp/sakura-w3-reproduce
mkdir -p "$W3_WORK"
python3 - <<'PYCODE'
import os
from pathlib import Path
s=Path('bench/programs/fib/sakura/main.sak').read_text()
s=s[:s.index('\npub fn main(')]+'\npub fn main() -> Unit ! {Console} {\n  let n = Option.expect(String.parse_int("30"), "size")\n  for _ in 0..12 { Console.print("{run(n)}") }\n}\n'
(Path(os.environ['W3_WORK'])/'fib.sak').write_text(s)
PYCODE
"$CARGO_TARGET_DIR/release/sakura" build --target wasm --wasi --no-direct --out-dir "$W3_WORK/fib-frame" "$W3_WORK/fib.sak"
"$CARGO_TARGET_DIR/release/sakura" build --target wasm --wasi --out-dir "$W3_WORK/fib-twin" "$W3_WORK/fib.sak"
```

The generated `main` follows `bench/harness/bench.py::wasmtime`. The input is
parsed at runtime to avoid a constant-folded workload. Both variants print
`832040` twelve times. The arithmetic and recursive `fib` body are unchanged.

## Measure without a profiler

Save this block as `$W3_WORK/measure.py` and run the command below it.
The first output includes startup and compilation and is reported separately.
Of the eleven intervals between twelve lines, discard the first two and retain
nine. Timing therefore excludes compilation and the initial calls.

```python
import datetime,json,os,statistics,subprocess,time
from pathlib import Path
root=Path(os.environ['W3_WORK'])
data={'engine':subprocess.check_output(['wasmtime','--version'],text=True).strip(),'n':30,'calls_per_process':12,'discarded_intervals':2,'quiet':False,'runs':[]}
for rnd in range(8):
 for arm in (['frame','twin'] if rnd%2==0 else ['twin','frame']):
  cmd=['wasmtime','run',str(root/('fib-'+arm)/'main.wasm')]
  t0=time.perf_counter_ns()
  p=subprocess.Popen(cmd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1)
  times=[]; checks=[]
  for line in p.stdout:
   times.append(time.perf_counter_ns());checks.append(line.strip())
  err=p.stderr.read();code=p.wait(timeout=120)
  assert code==0 and checks==['832040']*12,(cmd,code,checks,err)
  samples=[(b-a)/1e6 for a,b in zip(times,times[1:])][2:]
  data['runs'].append({'round':rnd,'arm':arm,'samples_ms':samples,'first_output_ms':(times[0]-t0)/1e6,'wall_ms':(time.perf_counter_ns()-t0)/1e6,'checksums':checks,'load':os.getloadavg(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()})
  (root/'w3-readings.json').write_text(json.dumps(data,indent=2)+'\n')
  print(rnd,arm,statistics.median(samples),flush=True)

```

```sh
python3 research/10-evidence/record_load.py --log "$W3_WORK/load.jsonl" -- python3 "$W3_WORK/measure.py"
```

## Collect the Linux perf profile

The reference host is macOS. The profile uses an isolated Ubuntu 24.04 container
on its OrbStack Linux VM. It is diagnostic evidence from a different execution
environment, not a reference-machine ceiling measurement. The retained
[Linux metadata](linux-host.txt) gives the image digest and kernel.

```sh
docker run -d --name sakura-w3-reproduce --cap-add PERFMON --security-opt seccomp=unconfined -v "$W3_WORK:/experiment" ubuntu@sha256:68ee22004a9109b44973313b02ff7b1de250f1de0f5ecb628d73f579aaff33ac sleep 3600
docker exec sakura-w3-reproduce sh -c 'apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq linux-tools-generic curl xz-utils ca-certificates'
docker exec sakura-w3-reproduce sh -c 'curl -fLsS https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.3/wasmtime-v49.0.3-aarch64-linux.tar.xz -o /tmp/wasmtime.tar.xz && tar -C /tmp -xf /tmp/wasmtime.tar.xz'
```

The measured perf executable was `/usr/lib/linux-tools/6.8.0-147-generic/perf`.
The `linux-tools-generic` package may install a newer version on a later date.
Resolve its installed executable before running these commands.

```sh
python3 research/10-evidence/record_load.py --log "$W3_WORK/perf-load.jsonl" -- docker exec sakura-w3-reproduce sh -c '/usr/lib/linux-tools/6.8.0-147-generic/perf record -F 997 -e cpu-clock -g -o /experiment/w3.perf -- /tmp/wasmtime-v49.0.3-aarch64-linux/wasmtime run --profile perfmap /experiment/fib-frame/main.wasm > /experiment/perf-output.txt 2> /experiment/wasmtime-stderr.txt'
docker exec sakura-w3-reproduce sh -c '/usr/lib/linux-tools/6.8.0-147-generic/perf report -i /experiment/w3.perf --stdio --no-children --show-nr-samples --sort dso,symbol --percent-limit 0 > /experiment/perf-report.txt'
docker exec sakura-w3-reproduce sh -c '/usr/lib/linux-tools/6.8.0-147-generic/perf script -i /experiment/w3.perf > /experiment/perf-script.txt'
```

Keep the perf-map file from the container's `/tmp/perf-*.map` until reports have
been generated. `perf record` includes startup and compilation. Report the raw
whole-process sample count and a separately labelled resolved-WASM denominator.
Do not add inclusive caller and callee percentages as if they were disjoint.

After saving the reports, stop and remove only the experiment's container.

```sh
docker stop sakura-w3-reproduce
docker rm sakura-w3-reproduce
```

## Read the structural counters

The counter run uses the original CLI benchmark once at n=30, separately from
the WASI timing wrapper. For `frame`, include `--no-direct`; for `twin`, omit it.
Use fresh stats paths because the host appends JSON lines.

```sh
"$CARGO_TARGET_DIR/release/sakura" build --target wasm --no-direct --out-dir "$W3_WORK/fib-stats-frame" bench/programs/fib/sakura/main.sak
SAKURA_WASM_STATS="$W3_WORK/frame-stats.jsonl" node "$W3_WORK/fib-stats-frame/sakura_wasm_host.js" "$W3_WORK/fib-stats-frame/main.wasm" 30
"$CARGO_TARGET_DIR/release/sakura" build --target wasm --out-dir "$W3_WORK/fib-stats-twin" bench/programs/fib/sakura/main.sak
SAKURA_WASM_STATS="$W3_WORK/twin-stats.jsonl" node "$W3_WORK/fib-stats-twin/sakura_wasm_host.js" "$W3_WORK/fib-stats-twin/main.wasm" 30
```

## Regenerate profile counts

Run this block after producing `perf-report.txt` and `perf-script.txt`.
For retained evidence, decompress the files with those names into `$W3_WORK`.
It deduplicates each symbol within a sampled stack before counting inclusive
samples, so recursive or duplicate stack entries are not counted twice.

```python
import collections, json, os, re
from pathlib import Path
root = Path(os.environ['W3_WORK'])
rows = []
for line in (root / 'perf-report.txt').read_text().splitlines():
    match = re.match(r'^\s+([0-9.]+)%\s+(\d+)\s+(.+?)\s+\[.\]\s+(.+?)\s+-\s+-\s*$', line)
    if match:
        rows.append({'percent_whole_process': float(match[1]), 'samples': int(match[2]),
                     'dso': match[3], 'symbol': match[4]})
blocks = (root / 'perf-script.txt').read_text().strip().split('\n\n')
counts = collections.Counter()
for block in blocks:
    symbols = set()
    for line in block.splitlines()[1:]:
        match = re.match(r'\s*[0-9a-f]+ (.*) \((.*)\)$', line)
        if match:
            symbols.add(re.sub(r'\+0x[0-9a-f]+$', '', match[1]))
    counts.update(symbols)
terms = ['frame_alloc', 'arena_new', 'pop_frame', '::after', '::drop',
         '::wasm::rt', '::wasm::dispatch', '::arena_obj']
summary = {
    'whole_process_samples': sum(row['samples'] for row in rows),
    'jit_samples': sum(row['samples'] for row in rows if '[JIT]' in row['dso']),
    'rows': rows,
    'inclusive_sample_counts': {key: value for key, value in counts.items()
                               if any(term in key for term in terms) or key == 'run'},
    'script_sample_blocks': len(blocks),
}
assert summary['whole_process_samples'] == len(blocks)
(root / 'profile-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
```

## Extract the emitted function-name section

This block reads only the emitted WASM metadata. It establishes which named
functions exist independently after compilation.

```python
import os
from pathlib import Path
root = Path(os.environ['W3_WORK'])
binary = (root / 'fib-frame/main.wasm').read_bytes()
def leb(data, offset):
    value = shift = 0
    while True:
        byte = data[offset]
        offset += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, offset
        shift += 7
def string(data, offset):
    length, offset = leb(data, offset)
    return data[offset:offset + length].decode(), offset + length
position = 8
names = []
while position < len(binary):
    kind = binary[position]
    length, start = leb(binary, position + 1)
    section = binary[start:start + length]
    position = start + length
    if kind != 0:
        continue
    name, offset = string(section, 0)
    if name != 'name':
        continue
    while offset < len(section):
        subsection = section[offset]
        length, start = leb(section, offset + 1)
        offset = start + length
        if subsection != 1:
            continue
        count, cursor = leb(section, start)
        for _ in range(count):
            index, cursor = leb(section, cursor)
            name, cursor = string(section, cursor)
            names.append((index, name))
(root / 'wasm-function-names.txt').write_text('\n'.join(f'{i} {n}' for i, n in names) + '\n')
```
