"""Generate review tables and load summaries without pooling RSS across cells."""
import json
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
summary = json.loads((HERE / 'summary.json').read_text())
cells = summary['cells']
sakura = [c for c in cells if c['variant'] == 'sakura']
lines = ['# Unchanged-build peak RSS report', '',
         'MB = 1,000,000 bytes. Increase = max/min − 1; width = (max − min)/median.',
         'All ranges are empirical. Follow-ups select pilot-volatile cells and all shapes cells; they are not a random population sample.', '',
         '## Pilot: worst individual-process spread per program', '',
         'The largest max/min − 1 among the four sizes, using only the five pilot processes in each cell.', '',
         '| Program | Node n | Node range MB | Node increase | Bun n | Bun range MB | Bun increase |',
         '|---|---:|---:|---:|---:|---:|---:|']
for program in sorted({c['program'] for c in sakura}):
    fields = [program]
    for engine in ('node', 'bun'):
        candidates = [c for c in sakura if c['program'] == program and c['engine'] == engine]
        c = max(candidates, key=lambda c: max(c['runs'][0]['peaks']) / min(c['runs'][0]['peaks']))
        peaks = c['runs'][0]['peaks']
        fields += [str(c['size']), f'{min(peaks)/1e6:.1f}–{max(peaks)/1e6:.1f}', f'{max(peaks)/min(peaks)-1:.1%}']
    lines.append('| ' + ' | '.join(fields) + ' |')
lines += ['', '## Shapes: all 15 processes, three groups of five', '',
          '| Engine | n | Individual peaks MB | Width / median | Individual increase | Group maxima MB | Maxima increase | Group medians MB | Medians increase |',
          '|---|---:|---:|---:|---:|---:|---:|---:|---:|']
for c in sakura:
    if c['program'] != 'shapes':
        continue
    p, ma, me = [c[f] for f in ('individual_peaks', 'run_maxima', 'run_medians')]
    lines.append(f"| {c['engine']} | {c['size']} | {p['min']/1e6:.1f}–{p['max']/1e6:.1f} | {p['width_over_median']:.1%} | {p['max_over_min_minus_one']:.1%} | {ma['min']/1e6:.1f}–{ma['max']/1e6:.1f} | {ma['max_over_min_minus_one']:.1%} | {me['min']/1e6:.1f}–{me['max']/1e6:.1f} | {me['max_over_min_minus_one']:.1%} |")
lines += ['', '## Fixed follow-ups: comparison sensitivity', '',
          'Counts below include both Sakura and hand JS, with six ordered run pairs per selected cell. Pairs reuse observations; these are not independent trials or false-positive rates.', '',
          '| Engine | Selected cells | Ordered pairs | Maximum >25% | Median >25% |',
          '|---|---:|---:|---:|---:|']
for engine in ('node', 'bun'):
    selected = [c for c in cells if c['engine']==engine and len(c['runs'])==3]
    lines.append('| ' + ' | '.join(map(str, [engine, len(selected), sum(c['ordered_pairs'] for c in selected), sum(c['max_pairs_over_25pct'] for c in selected), sum(c['median_pairs_over_25pct'] for c in selected)])) + ' |')
lines += ['', '## Complete per-cell process distributions', '',
          '| Program | Variant | Engine | n | Processes | Min MB | Median MB | Max MB | Width / median | Max / min − 1 |',
          '|---|---|---|---:|---:|---:|---:|---:|---:|---:|']
for c in cells:
    p = c['individual_peaks']
    lines.append(f"| {c['program']} | {c['variant']} | {c['engine']} | {c['size']} | {sum(len(r['peaks']) for r in c['runs'])} | {p['min']/1e6:.2f} | {p['median']/1e6:.2f} | {p['max']/1e6:.2f} | {p['width_over_median']:.1%} | {p['max_over_min_minus_one']:.1%} |")
(HERE / 'report.md').write_text('\n'.join(lines) + '\n')
loads = []
for path in sorted(HERE.glob('*-load.jsonl')):
    samples = [json.loads(line) for line in path.read_text().splitlines()]
    cpu = [r['sum_process_cpu_percent'] for r in samples]
    one = [r['load_average_1_5_15'][0] for r in samples]
    loads.append({'file': path.name, 'samples': len(samples), 'start': samples[0]['utc'], 'end': samples[-1]['utc'],
                  'exit_code': samples[-1].get('exit_code'), 'elapsed_seconds': samples[-1].get('elapsed_seconds'),
                  'logical_cpus': samples[0]['logical_cpus'],
                  'load_1min': {'min': min(one), 'median': statistics.median(one), 'max': max(one)},
                  'summed_cpu_percent': {'min': min(cpu), 'median': statistics.median(cpu), 'max': max(cpu)},
                  'busy_executables': sorted({p['executable'] for s in samples for p in s['top_cpu'][:4]})})
(HERE / 'load-summary.json').write_text(json.dumps(loads, indent=2) + '\n')
